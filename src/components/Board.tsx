import { Fragment, useEffect, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode, TouchEvent } from "react";
import { STAGES, type Interest, type OfferRow, type View } from "../api/types";
import { plural, useI18n, type TextKey } from "../i18n";
import { addedLabel, companyLabel, placeLabel, salaryShort, shortDay, translatedTitle, shownScore } from "../lib/format";
import { isNarrowLayout } from "../lib/fit";
import { onBack } from "../lib/back";
import { isPhone } from "../lib/platform";
import { motionReduced } from "../lib/presence";
import { isBin, reasonText, sortedReasons } from "../lib/profile";
import { SWIPE_AT, isSwipe, lockAxis, swipeOffset, type SwipeAxis } from "../lib/swipe";
import { Flap } from "./Flap";
import { IconStar } from "./Icons";
import { StatusTag } from "./StatusTag";

export type SortKey = "added" | "offer" | "place" | "salary" | "match" | "status";
export interface Sort {
  key: SortKey;
  dir: "asc" | "desc";
}

/** Every tab but "Today": the newest published first. */
export const DEFAULT_SORT: Sort = { key: "added", dir: "desc" };

const COLUMNS: { key: SortKey; label: TextKey; className: string }[] = [
  { key: "added", label: "col.added", className: "c-added" },
  { key: "offer", label: "col.offer", className: "c-offer" },
  { key: "place", label: "col.place", className: "c-place" },
  { key: "salary", label: "col.salary", className: "c-salary" },
  { key: "match", label: "col.match", className: "c-match" },
  { key: "status", label: "col.status", className: "c-status" },
];

/** Finished offers - rejected, withdrawn, or (outside a recruitment) expired or probably expired.
 *  Kept for later, greyed out at the bottom. A recruitment never goes there: it only gets a mark. */
export function isClosed(o: OfferRow): boolean {
  return o.stage === "rejected" || o.stage === "withdrawn" || ((o.expired || o.stale) && !o.inRecruitment);
}

/** Sorted by the chosen column, with finished offers always in their own group below the rest. */
export function sortRows(rows: OfferRow[], sort: Sort): OfferRow[] {
  const value = (o: OfferRow): string | number => {
    switch (sort.key) {
      case "added":
        return o.publishedAt;
      case "offer":
        return `${o.title} ${o.company}`.toLocaleLowerCase();
      case "place":
        return (o.remote === "yes" ? "" : (o.city ?? "~")).toLocaleLowerCase();
      case "salary":
        return o.salaryMax ?? o.salaryMin ?? -1;
      case "match":
        return shownScore(o) ?? -1;
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
  /** Instead of the tab's own "no offers" text - e.g. none from the chosen portal. */
  emptyText?: string;
  view: View;
  selectedId: number | null;
  freshIds: Set<number>;
  /** `null` = the order the list came in ("Today", ranked by Rust). */
  sort: Sort | null;
  onSort: (key: SortKey) => void;
  onSelect: (id: number) => void;
  onOpen: () => void;
  onInterest: (id: number, interest: Interest) => void;
  onTrash: (id: number) => void;
  onExpired: (id: number) => void;
  /** "To own" (key D) - only for a collected offer. */
  onOwn: (id: number) => void;
  /** "Restore" in the bin (key P, in English R). */
  onRestore: (id: number) => void;
  /** Quick browsing: the next offer after a decision made with a key. */
  onNext: (id: number) => void;
}

/** How long the children slide out when the tree folds (as `tree-out` in app.css). */
const TREE_OUT_MS = 160;

export function Board({ rows, emptyText, view, selectedId, freshIds, sort, onSort, onSelect, onOpen, onInterest, onTrash, onExpired, onOwn, onRestore, onNext }: Props) {
  const touch = useRef<{ id: number; x: number; y: number; axis: SwipeAxis | null } | null>(null);
  /** The row whose interest mark just came from a swipe - the mark flips in. */
  const [flipped, setFlipped] = useState<number | null>(null);
  const { t, lang } = useI18n();
  /** Parents shown with their children - the tree starts folded (only the first ads). */
  const [unfolded, setUnfolded] = useState<Set<number>>(new Set());
  /** Parents folding right now - their children slide out before they go (not with reduced motion). */
  const [folding, setFolding] = useState<Set<number>>(new Set());
  const change = (set: typeof setUnfolded, id: number, on: boolean) =>
    set((now) => {
      const next = new Set(now);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });
  const fold = (id: number, open: boolean) => {
    change(setFolding, id, false);
    if (open || document.documentElement.dataset.motion === "reduced") {
      change(setUnfolded, id, open);
      return;
    }
    change(setFolding, id, true);
    window.setTimeout(() => {
      // Unfolded again meanwhile - it stays open.
      setFolding((now) => {
        if (now.has(id)) change(setUnfolded, id, false);
        const next = new Set(now);
        next.delete(id);
        return next;
      });
    }, TREE_OUT_MS);
  };
  /** Open on the screen: unfolded and not folding. */
  const isOpen = (id: number) => unfolded.has(id) && !folding.has(id);
  // Android "back" folds the tree unfolded last (one on this list - another tab's stays as it was).
  const lastOpen = [...unfolded].filter((id) => !folding.has(id) && rows.some((r) => r.id === id)).pop();
  const foldBack = (id: number) => {
    // A chosen child would open its parent again at once - the parent becomes the chosen row.
    if (rows.find((r) => r.id === id)?.children.some((c) => c.id === selectedId)) onSelect(id);
    fold(id, false);
  };
  const foldRef = useRef(foldBack);
  foldRef.current = foldBack;
  useEffect(() => (lastOpen !== undefined ? onBack(() => foldRef.current(lastOpen), true) : undefined), [lastOpen]);
  /** Rows as they stand on the screen: every parent, its children when unfolded. */
  const shown = rows.flatMap((r) => [r, ...(unfolded.has(r.id) ? r.children : [])]);
  // A child chosen elsewhere (the card's list of ads) - its parent opens to show it.
  const chosenChildOf = rows.find((r) => r.children.some((c) => c.id === selectedId));
  if (chosenChildOf && !unfolded.has(chosenChildOf.id)) fold(chosenChildOf.id, true);
  const index = shown.findIndex((r) => r.id === selectedId);
  // The bin: the last thrown in on top - no group of finished offers there.
  const firstClosed = isBin(view) ? -1 : rows.findIndex(isClosed);
  const closedCount = firstClosed < 0 ? 0 : rows.length - firstClosed;

  function onKeyDown(e: KeyboardEvent) {
    if (e.target !== e.currentTarget || shown.length === 0) return;
    const go = (i: number) => {
      const row = shown[Math.max(0, Math.min(shown.length - 1, i))];
      onSelect(row.id);
      document.getElementById(`offer-${row.id}`)?.scrollIntoView({ block: "nearest" });
    };
    const interests: Record<string, Interest> = { "1": "high", "2": "maybe", "3": "no" };
    // The bin only restores (the card can delete for good, after a question).
    const editable = selectedId !== null && !isBin(view);
    const plain = !e.ctrlKey && !e.metaKey && !e.altKey;
    const current = shown[index];
    if (e.key === "ArrowDown") go(index + 1);
    // The tree: right unfolds the other ads, left folds them (from a child - back to its parent).
    else if (e.key === "ArrowRight" && current && current.children.length > 0 && !isOpen(current.id)) fold(current.id, true);
    else if (e.key === "ArrowLeft" && current?.parentId != null) go(shown.findIndex((r) => r.id === current.parentId));
    else if (e.key === "ArrowLeft" && current && isOpen(current.id)) fold(current.id, false);
    else if (e.key.toLowerCase() === (lang === "pl" ? "p" : "r") && plain && selectedId !== null && isBin(view)) onRestore(selectedId);
    else if (e.key === "ArrowUp") go(index < 0 ? 0 : index - 1);
    else if (e.key === "Home") go(0);
    else if (e.key === "End") go(shown.length - 1);
    else if (e.key === "Enter" && selectedId !== null) onOpen();
    else if (interests[e.key] && editable) {
      // A decision made - straight on to the next offer.
      onInterest(selectedId, interests[e.key]);
      onNext(selectedId);
    } else if ((e.key === "w" || e.key === "W") && !e.ctrlKey && !e.metaKey && !e.altKey && editable) onExpired(selectedId);
    else if (e.key === "Delete" && editable) onTrash(selectedId);
    else if ((e.key === "d" || e.key === "D") && !e.ctrlKey && !e.metaKey && !e.altKey && editable && !shown[index]?.own) onOwn(selectedId);
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
        <p>{emptyText ?? t(view === "board" && isPhone() ? "board.empty.phone" : `board.empty.${view}`)}</p>
      </div>
    );
  }

  return (
    <div className="board-wrap">
      <table
        className="board"
        role="treegrid"
        aria-label={t("board.label")}
        aria-rowcount={shown.length + 1 + (closedCount > 0 ? 1 : 0)}
        aria-activedescendant={selectedId !== null ? `offer-${selectedId}` : undefined}
        tabIndex={0}
        onKeyDown={onKeyDown}
      >
        <thead role="rowgroup">
          <tr role="row">
            {COLUMNS.map((col) => {
              const active = sort?.key === col.key;
              return (
                <th
                  key={col.key}
                  role="columnheader"
                  className={col.className}
                  scope="col"
                  aria-sort={active ? (sort?.dir === "asc" ? "ascending" : "descending") : "none"}
                >
                  <button type="button" className="sort" onClick={() => onSort(col.key)}>
                    {col.key === "match" ? (
                      <abbr title={t("col.matchLong")}>{t(col.label)}</abbr>
                    ) : col.key === "added" ? (
                      <abbr title={t("col.addedLong")}>{t(col.label)}</abbr>
                    ) : (
                      t(col.label)
                    )}
                    <span className="sort-mark" aria-hidden="true">
                      {active ? (sort?.dir === "asc" ? "▲" : "▼") : ""}
                    </span>
                  </button>
                </th>
              );
            })}
          </tr>
        </thead>
        <tbody role="rowgroup">
          {rows.map((parent, i) => [parent, ...(unfolded.has(parent.id) ? parent.children : [])].map((o) => {
            const child = o.parentId !== null && o.id !== parent.id;
            const fresh = freshIds.has(o.id);
            const selected = o.id === selectedId;
            const sourceNames = o.sources.map((s) => sourceLabel(s, t));
            return (
              <Fragment key={o.id}>
              {i === firstClosed && !child && (
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
                aria-level={child ? 2 : 1}
                aria-expanded={!child && o.children.length > 0 ? isOpen(o.id) : undefined}
                className={[
                  selected && "is-selected",
                  child && "is-child",
                  // The tree folds and unfolds smoothly - the children slide in and out.
                  child && (folding.has(parent.id) ? "tree-out" : "tree-in"),
                  // The person's own offer: the yellow stripe everywhere (in "Own" every row has it).
                  o.own && "is-own",
                  // In the bin every row is thrown out - its mark says why, no greying.
                  o.interest === "no" && !isBin(view) && "is-dim",
                  (o.expired || o.stale) && !o.inRecruitment && !isBin(view) && "is-expired",
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
                // No rating in the bin - like the keys.
                onTouchStart={(e) => (touch.current = isBin(view) ? null : { id: o.id, x: e.touches[0].clientX, y: e.touches[0].clientY, axis: null })}
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
                  <Fold on={child}>
                    <CellName text={t("col.addedLong")} />
                    <Flap className="mono" text={addedLabel(o.publishedAt, lang, t)} fresh={fresh} />
                    {/* Under the date, in the first column: the ads below are a step in from here.
                        "+1 more" - as many rows as unfold (the first ad is this row). */}
                    {!child && o.children.length > 0 && (
                      <button
                        type="button"
                        className="tree-toggle"
                        aria-expanded={isOpen(o.id)}
                        title={t("tree.moreLabel", { count: o.children.length })}
                        onClick={(e) => {
                          // Folding is not choosing the row.
                          e.stopPropagation();
                          fold(o.id, !isOpen(o.id));
                        }}
                      >
                        {/* One arrow that turns down - smoothly, with the children. */}
                        <span className={`tree-mark${isOpen(o.id) ? " is-open" : ""}`} aria-hidden="true">
                          ▸
                        </span>
                        {t(plural(lang, "tree.more", o.children.length), { count: o.children.length })}
                      </button>
                    )}
                  </Fold>
                </td>
                <td role="gridcell" className="c-offer">
                  <Fold on={child}>
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
                      {/* A child: which ad it is - the portal says it better than the repeated company. */}
                      {child ? (
                        <span className="offer-company">{t("tree.childOf", { source: sourceNames.join(" · ") })}</span>
                      ) : (
                        <span className="offer-company">{companyLabel(o.company, t)}</span>
                      )}
                      {o.interest && (
                        <span key={o.interest} className={`interest interest-${o.interest}${flipped === o.id ? " flap-in" : ""}`}>
                          <span className="sr-only">{t("interest.label")}: </span>
                          {t(`interest.${o.interest}`)}
                        </span>
                      )}
                    </span>
                    {child ? null : o.sources.length > 1 ? (
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
                    {/* "OWN" only where own and collected offers mix (the bin) - in "Own" every one is.
                        In the bin also why the offer is there, in words. */}
                    {((o.own && view !== "own") || o.noLink || o.renewAsked || o.dupAsk || o.sentVia || (isBin(view) && o.bin)) && (
                      <span className="offer-line row-marks">
                        {isBin(view) && o.bin && <span className={`row-mark row-mark-bin row-mark-${o.bin}`}>{t(`row.bin.${o.bin}`)}</span>}
                        {o.own && view !== "own" && <span className="row-mark row-mark-own">{t("row.own")}</span>}
                        {o.noLink && <span className="row-mark row-mark-nolink">{t("row.noLink")}</span>}
                        {o.renewAsked && <span className="row-mark row-mark-renewed">{t("row.renewed")}</span>}
                        {o.dupAsk && <span className="row-mark row-mark-dup">{t("row.dup")}</span>}
                        {/* The CV went through a child: the parent keeps its own stage, the family is one recruitment. */}
                        {o.sentVia && (
                          <span className="row-mark row-mark-sent">
                            {t("tree.sentVia", { source: sourceLabel(o.sentVia.source, t), date: shortDay(o.sentVia.at, lang) })}
                          </span>
                        )}
                      </span>
                    )}
                    {/* AI orders "Today" and never hides: one sentence says why the score. */}
                    {view === "today" && o.scoreSource === "ai" && o.scoreWhy && (
                      <span className="offer-why">{t("row.why", { why: o.scoreWhy })}</span>
                    )}
                  </Fold>
                </td>
                <td role="gridcell" className="c-place">
                  <Fold on={child}>
                    <CellName text={t("cell.place")} />
                    <Flap text={placeLabel(o, t)} fresh={fresh} />
                  </Fold>
                </td>
                <td role="gridcell" className="c-salary">
                  <Fold on={child}>
                    <CellName text={t("col.salary")} />
                    <Flap className="mono" text={salaryShort(o, lang, t)} fresh={fresh} />
                  </Fold>
                </td>
                <td role="gridcell" className="c-match">
                  <Fold on={child}>
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
                        <Flap className="mono" text={String(shownScore(o))} fresh={fresh} />
                        {o.scoreSource === "keywords" && <span className="sr-only"> ({t("card.matchKeywords")})</span>}
                      </span>
                    )}
                  </Fold>
                </td>
                <td role="gridcell" className="c-status">
                  <Fold on={child}>
                    <StatusTag offer={o} fresh={fresh} />
                  </Fold>
                </td>
              </tr>
              </Fragment>
            );
          }))}
        </tbody>
      </table>
    </div>
  );
}

/** Another ad's cell: its content unfolds in height (a table row cannot), so the rows below
 *  slide down with it instead of jumping. */
function Fold({ on, children }: { on: boolean; children: ReactNode }) {
  if (!on) return <>{children}</>;
  return (
    <div className="tree-fold">
      <div className="tree-fold-inner">{children}</div>
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
