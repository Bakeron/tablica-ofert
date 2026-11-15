import { useEffect, useId, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import type { OfferRow } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { onBack } from "../lib/back";
import { applyFilters, FILTER_KEYS, filterCounts, MULTI_KEYS, type FilterKey, type ListFilters } from "../lib/portals";
import { sourceLabel } from "./Board";

const LABEL: Record<FilterKey, TextKey> = {
  portal: "portal.label",
  status: "col.status",
  mode: "narrow.modes",
  city: "narrow.cities",
  salary: "col.salary",
  english: "narrow.english",
};

/** The filters worth showing: the whole list has more than one value for them, or one is chosen. */
export function availableFilters(rows: OfferRow[], filters: ListFilters) {
  return FILTER_KEYS.map((key) => ({ key, options: filterCounts(rows, filters, key) })).filter((f) => f.options.length > 1 || filters[f.key].length > 0);
}

/** The choices above every list - the name above, the choice below; they wrap to the width, centred.
 *  Status is one choice (a list); the others are tick boxes - none ticked = all. */
export function ListFilterBar({ rows, filters, onChange }: { rows: OfferRow[]; filters: ListFilters; onChange: (next: ListFilters) => void }) {
  const { t } = useI18n();
  const name = (key: FilterKey, value: string): string => {
    switch (key) {
      case "portal":
        return sourceLabel(value, t);
      case "status":
        return t(`stage.${value}` as TextKey);
      case "mode":
        return t(`remote.${value}` as TextKey);
      case "city":
        return value;
      case "salary":
        return t(`narrow.salary.${value}` as TextKey);
      case "english":
        return t(`narrow.english.${value}` as TextKey);
    }
  };
  const shown = availableFilters(rows, filters);
  if (shown.length === 0) return null;
  return (
    <div className="list-filters" id="list-filters">
      {shown.map(({ key, options }) => {
        const chosen = filters[key];
        // "all (N)": the offers the other filters leave.
        const all = t("portal.all", { count: applyFilters(rows, filters, key).length });
        const set = (values: string[]) => onChange({ ...filters, [key]: values });
        if (!MULTI_KEYS.includes(key)) {
          return (
            <label key={key} className="portal-filter">
              <span>{t(LABEL[key])}</span>
              <select value={chosen[0] ?? ""} onChange={(e) => set(e.target.value === "" ? [] : [e.target.value])}>
                <option value="">{all}</option>
                {options.map((o) => (
                  <option key={o.value} value={o.value}>
                    {name(key, o.value)} ({o.count})
                  </option>
                ))}
              </select>
            </label>
          );
        }
        return (
          <MultiPick
            key={key}
            label={t(LABEL[key])}
            summary={chosen.length === 0 ? all : chosen.map((v) => name(key, v)).join(", ")}
            options={options.map((o) => ({ value: o.value, text: `${name(key, o.value)} (${o.count})` }))}
            chosen={chosen}
            onChange={set}
          />
        );
      })}
    </div>
  );
}

/** A choice that looks like a list but opens tick boxes - several at once; closes on a click
 *  outside, Esc, Tab out of it or "back" on a phone. */
function MultiPick({
  label,
  summary,
  options,
  chosen,
  onChange,
}: {
  label: string;
  summary: string;
  options: { value: string; text: string }[];
  chosen: string[];
  onChange: (values: string[]) => void;
}) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLFieldSetElement>(null);
  const id = useId();
  /** The panel floats over the list (fixed, so the folding filters never clip it), under its field. */
  const [place, setPlace] = useState<CSSProperties>({});
  useLayoutEffect(() => {
    if (!open || !button.current) return;
    const r = button.current.getBoundingClientRect();
    const width = panel.current?.offsetWidth ?? r.width;
    // Kept inside the window on a narrow screen.
    setPlace({ top: r.bottom + 4, left: Math.max(8, Math.min(r.left, window.innerWidth - width - 8)), minWidth: r.width });
  }, [open]);
  useEffect(() => {
    if (!open) return;
    const outside = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    // The list scrolling or the window resizing would leave the panel behind - it closes.
    const moved = (e: Event) => {
      if (!panel.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", outside);
    window.addEventListener("scroll", moved, true);
    window.addEventListener("resize", moved);
    const back = onBack(() => setOpen(false));
    return () => {
      document.removeEventListener("pointerdown", outside);
      window.removeEventListener("scroll", moved, true);
      window.removeEventListener("resize", moved);
      back();
    };
  }, [open]);
  return (
    <div
      className="portal-filter multi-pick"
      ref={box}
      onKeyDown={(e) => {
        if (e.key !== "Escape" || !open) return;
        e.stopPropagation();
        setOpen(false);
        button.current?.focus();
      }}
      onBlur={(e) => {
        if (e.relatedTarget && !box.current?.contains(e.relatedTarget as Node)) setOpen(false);
      }}
    >
      <span id={`${id}-label`}>{label}</span>
      <button
        ref={button}
        type="button"
        className="multi-pick-button"
        aria-expanded={open}
        aria-controls={`${id}-panel`}
        aria-labelledby={`${id}-label ${id}-summary`}
        onClick={() => setOpen(!open)}
      >
        <span id={`${id}-summary`} className="multi-pick-summary">
          {summary}
        </span>
        <span className="multi-pick-arrow" aria-hidden="true">
          ▾
        </span>
      </button>
      {open && (
        <fieldset id={`${id}-panel`} ref={panel} className="multi-pick-panel" style={place}>
          <legend className="sr-only">{label}</legend>
          {options.map((o) => (
            <label key={o.value} className="choice-item">
              <input
                type="checkbox"
                checked={chosen.includes(o.value)}
                onChange={(e) => onChange(e.target.checked ? [...chosen, o.value] : chosen.filter((v) => v !== o.value))}
              />
              <span>{o.text}</span>
            </label>
          ))}
          {chosen.length > 0 && (
            <button type="button" className="btn btn-ghost btn-small" onClick={() => onChange([])}>
              {t("narrow.clear")}
            </button>
          )}
        </fieldset>
      )}
    </div>
  );
}
