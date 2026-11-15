import type { OfferRow } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { applyFilters, FILTER_KEYS, filterCounts, type FilterKey, type ListFilters } from "../lib/portals";
import { sourceLabel } from "./Board";

const LABEL: Record<FilterKey, TextKey> = {
  portal: "portal.label",
  status: "col.status",
  place: "col.place",
  salary: "col.salary",
  english: "narrow.english",
};

/** The choices above every list - one each, shown when the list has more than one value or one is chosen. */
export function ListFilterBar({ rows, filters, onChange }: { rows: OfferRow[]; filters: ListFilters; onChange: (next: ListFilters) => void }) {
  const { t } = useI18n();
  const name = (key: FilterKey, value: string): string => {
    switch (key) {
      case "portal":
        return sourceLabel(value, t);
      case "status":
        return t(`stage.${value}` as TextKey);
      case "place":
        return value.startsWith("remote:") ? t(`remote.${value.slice(7)}` as TextKey) : value.slice(5);
      case "salary":
        return t(`narrow.salary.${value}` as TextKey);
      case "english":
        return t(`narrow.english.${value}` as TextKey);
    }
  };
  const shown = FILTER_KEYS.map((key) => ({ key, options: filterCounts(rows, filters, key) })).filter((f) => f.options.length > 1 || filters[f.key] !== null);
  if (shown.length === 0) return null;
  return (
    <div className="list-filters">
      {shown.map(({ key, options }) => {
        const option = (o: { value: string; count: number }) => (
          <option key={o.value} value={o.value}>
            {name(key, o.value)} ({o.count})
          </option>
        );
        const modes = options.filter((o) => o.value.startsWith("remote:"));
        const cities = options.filter((o) => o.value.startsWith("city:"));
        return (
          <label key={key} className="portal-filter">
            <span>{t(LABEL[key])}</span>
            <select value={filters[key] ?? ""} onChange={(e) => onChange({ ...filters, [key]: e.target.value === "" ? null : e.target.value })}>
              {/* "all (N)": the offers the other filters leave. */}
              <option value="">{t("portal.all", { count: applyFilters(rows, filters, key).length })}</option>
              {/* City / remote: the work modes and the cities as two groups. */}
              {key === "place" && modes.length > 0 && cities.length > 0 ? (
                <>
                  <optgroup label={t("narrow.modes")}>{modes.map(option)}</optgroup>
                  <optgroup label={t("narrow.cities")}>{cities.map(option)}</optgroup>
                </>
              ) : (
                options.map(option)
              )}
            </select>
          </label>
        );
      })}
    </div>
  );
}
