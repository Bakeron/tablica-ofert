import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import type {
  Contract,
  Criteria,
  HardFlags,
  Level,
  OfferLanguage,
  PayBasis,
  Profile,
  ProfileInput,
  Remote,
  SalaryPeriod,
  SalaryRule,
} from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { emptyCriteria, joinList, splitList, toAmount } from "../lib/profile";
import { SuggestPanel, TidyDialog, suggestionsFrom, type Suggestion } from "./AiDialogs";
import { DescriptionField } from "./DescriptionField";
import { Help } from "./Help";
import { IconClose, IconPlus, IconTrash } from "./Icons";
import { Modal } from "./Modal";

const RADII = [0, 10, 20, 30, 50, 100];
const MODES: Remote[] = ["yes", "hybrid", "no"];
const CONTRACTS: Contract[] = ["uop", "b2b", "mandate"];
const LEVELS: Level[] = ["junior", "mid", "senior"];
const LANGUAGES: OfferLanguage[] = ["pl", "en"];

interface SalaryForm {
  min: string;
  expected: string;
  basis: PayBasis;
  period: SalaryPeriod;
}

interface Form {
  name: string;
  description: string;
  experience: string;
  keywords: string;
  excludeWords: string;
  skipCompanies: string;
  workModes: Remote[];
  cities: { name: string; radiusKm: number }[];
  commute: number | null;
  contracts: Contract[];
  levels: Level[];
  languages: OfferLanguage[];
  uop: SalaryForm;
  b2b: SalaryForm;
  hard: HardFlags;
}

const amount = (v: number | null) => (v === null ? "" : String(v));
const salaryForm = (r: SalaryRule): SalaryForm => ({ min: amount(r.min), expected: amount(r.expected), basis: r.basis, period: r.period });
const salaryRule = (f: SalaryForm): SalaryRule => ({ min: toAmount(f.min), expected: toAmount(f.expected), basis: f.basis, period: f.period });

export function toForm(profile: Profile | null, experience: string, defaultName = ""): Form {
  const c: Criteria = profile?.criteria ?? emptyCriteria();
  return {
    name: profile?.name ?? defaultName,
    description: profile?.description ?? "",
    experience,
    keywords: joinList(c.keywords),
    excludeWords: joinList(c.excludeWords),
    skipCompanies: joinList(c.skipCompanies),
    workModes: c.workModes,
    cities: c.cities.map((x) => ({ ...x })),
    commute: c.commutePerMonth,
    contracts: c.contracts,
    levels: c.levels,
    languages: c.languages,
    uop: salaryForm(c.salaryUop),
    b2b: salaryForm(c.salaryB2b),
    hard: { ...c.hard },
  };
}

export function toInput(form: Form, id: number | null): ProfileInput {
  return {
    id,
    name: form.name,
    description: form.description,
    criteria: {
      keywords: splitList(form.keywords),
      workModes: form.workModes,
      cities: form.cities.filter((c) => c.name.trim() !== "").map((c) => ({ name: c.name.trim(), radiusKm: c.radiusKm })),
      commutePerMonth: form.commute,
      contracts: form.contracts,
      levels: form.levels,
      languages: form.languages,
      salaryUop: salaryRule(form.uop),
      salaryB2b: salaryRule(form.b2b),
      skipCompanies: splitList(form.skipCompanies),
      excludeWords: splitList(form.excludeWords),
      hard: form.hard,
    },
  };
}

function toggle<T>(list: T[], value: T): T[] {
  return list.includes(value) ? list.filter((v) => v !== value) : [...list, value];
}

export function ProfileDialog({
  open,
  profile,
  experience,
  experiencePrev,
  aiOn,
  onSuggest,
  onTidy,
  onClose,
  onSave,
  onDelete,
}: {
  open: boolean;
  /** null = new profile */
  profile: Profile | null;
  experience: string;
  /** Experience before the last change - can be brought back. */
  experiencePrev: string | null;
  /** An AI provider is set - "Tidy up" and "Suggest filters" are offered. */
  aiOn: boolean;
  onSuggest: (description: string) => Promise<Criteria | null>;
  onTidy: (text: string) => Promise<string | null>;
  onClose: () => void;
  onSave: (input: ProfileInput, experience: string) => Promise<boolean>;
  onDelete: (profile: Profile) => void;
}) {
  const { t } = useI18n();
  const [form, setForm] = useState<Form>(() => toForm(profile, experience));
  const [busy, setBusy] = useState(false);
  const [aiBusy, setAiBusy] = useState<"description" | "experience" | "suggest" | null>(null);
  const [tidy, setTidy] = useState<{ field: "description" | "experience"; before: string; proposal: string } | null>(null);
  const [suggestions, setSuggestions] = useState<Suggestion[] | null>(null);

  useEffect(() => {
    if (open) {
      setForm(toForm(profile, experience));
      setSuggestions(null);
      setTidy(null);
    }
  }, [open, profile, experience]);

  async function askTidy(field: "description" | "experience") {
    setAiBusy(field);
    const proposal = await onTidy(form[field]);
    setAiBusy(null);
    if (proposal) setTidy({ field, before: form[field], proposal });
  }

  async function askSuggest() {
    setAiBusy("suggest");
    const c = await onSuggest(form.description);
    setAiBusy(null);
    if (c) setSuggestions(suggestionsFrom(c, suggestionLabel));
  }

  function suggestionLabel(group: Suggestion["group"], value: string): string {
    const what = t(`suggest.g.${group}` as TextKey);
    const shown =
      group === "workModes" ? t(`remote.${value}` as TextKey)
      : group === "contracts" ? t(`contract.${value}` as TextKey)
      : group === "levels" ? t(`level.${value}` as TextKey)
      : group === "languages" ? t(`language.${value}` as TextKey)
      : value;
    return `${what}: ${shown}`;
  }

  function isPresent(s: Suggestion): boolean {
    const has = (list: string[]) => list.some((v) => v.toLowerCase() === s.value.toLowerCase());
    switch (s.group) {
      case "keywords":
        return has(splitList(form.keywords));
      case "excludeWords":
        return has(splitList(form.excludeWords));
      case "cities":
        return has(form.cities.map((c) => c.name));
      default:
        return (form[s.group] as string[]).includes(s.value);
    }
  }

  function addSuggestions(chosen: Suggestion[]) {
    setForm((f) => {
      const next = { ...f };
      const pick = (g: Suggestion["group"]) => chosen.filter((s) => s.group === g).map((s) => s.value);
      next.keywords = joinList([...splitList(f.keywords), ...pick("keywords")]);
      next.excludeWords = joinList([...splitList(f.excludeWords), ...pick("excludeWords")]);
      next.cities = [...f.cities, ...pick("cities").map((name) => ({ name, radiusKm: 20 }))];
      next.workModes = [...new Set([...f.workModes, ...(pick("workModes") as Remote[])])];
      next.contracts = [...new Set([...f.contracts, ...(pick("contracts") as Contract[])])];
      next.levels = [...new Set([...f.levels, ...(pick("levels") as Level[])])];
      next.languages = [...new Set([...f.languages, ...(pick("languages") as OfferLanguage[])])];
      return next;
    });
    setSuggestions(null);
  }

  const set = <K extends keyof Form>(key: K, value: Form[K]) => setForm((f) => ({ ...f, [key]: value }));
  const setHard = (key: keyof HardFlags, value: boolean) => setForm((f) => ({ ...f, hard: { ...f.hard, [key]: value } }));
  const setSalary = (which: "uop" | "b2b", patch: Partial<SalaryForm>) =>
    setForm((f) => ({ ...f, [which]: { ...f[which], ...patch } }));

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    const ok = await onSave(toInput(form, profile?.id ?? null), form.experience);
    setBusy(false);
    if (ok) onClose();
  }

  const hard = (key: keyof HardFlags, what: TextKey) => (
    <HardSwitch value={form.hard[key]} label={t("profile.hardLabel", { what: t(what) })} onChange={(v) => setHard(key, v)} />
  );

  return (
    <Modal open={open} onClose={onClose} labelledBy="profile-title" className="modal-wide">
      <form onSubmit={submit} className="sheet">
        <header className="sheet-head">
          <h2 id="profile-title">{profile ? t("profile.titleEdit", { name: profile.name }) : t("profile.titleNew")}</h2>
          <button type="button" className="icon-btn" onClick={onClose} aria-label={t("add.cancel")}>
            <IconClose />
          </button>
        </header>
        <p className="hint">{t("profile.intro")}</p>

        <label className="field field-name">
          <span>
            {t("profile.name")} <em className="req">({t("add.required")})</em>
          </span>
          <input
            required
            maxLength={40}
            placeholder={t("profile.namePlaceholder")}
            value={form.name}
            onChange={(e) => set("name", e.target.value)}
            data-autofocus
          />
        </label>

        <DescriptionField value={form.description} onChange={(v) => set("description", v)} />
        {(aiOn || (profile?.descriptionPrev && profile.descriptionPrev !== form.description)) && (
          <div className="ai-row">
            {aiOn && (
              <>
                <button type="button" className="btn btn-ghost btn-small" disabled={!form.description.trim() || aiBusy !== null} aria-busy={aiBusy === "description"} onClick={() => askTidy("description")}>
                  {aiBusy === "description" ? t("ai.working") : t("tidy.button")}
                </button>
                <button type="button" className="btn btn-ghost btn-small" disabled={!form.description.trim() || aiBusy !== null} aria-busy={aiBusy === "suggest"} onClick={askSuggest}>
                  {aiBusy === "suggest" ? t("ai.working") : t("suggest.button")}
                </button>
              </>
            )}
            {profile?.descriptionPrev && profile.descriptionPrev !== form.description && (
              <button type="button" className="btn btn-ghost btn-small" onClick={() => set("description", profile.descriptionPrev!)}>
                {t("tidy.restore")}
              </button>
            )}
          </div>
        )}
        {suggestions && <SuggestPanel items={suggestions} present={isPresent} onAdd={addSuggestions} onClose={() => setSuggestions(null)} />}

        <label className="field">
          <span>{t("profile.experience")}</span>
          <textarea
            rows={3}
            maxLength={20000}
            aria-describedby="experience-hint"
            value={form.experience}
            onChange={(e) => set("experience", e.target.value)}
          />
          <span id="experience-hint" className="hint">
            {t("profile.experienceHint")}
          </span>
        </label>
        {(aiOn || (experiencePrev !== null && experiencePrev !== form.experience)) && (
          <div className="ai-row">
            {aiOn && (
              <button type="button" className="btn btn-ghost btn-small" disabled={!form.experience.trim() || aiBusy !== null} aria-busy={aiBusy === "experience"} onClick={() => askTidy("experience")}>
                {aiBusy === "experience" ? t("ai.working") : t("tidy.button")}
              </button>
            )}
            {experiencePrev !== null && experiencePrev !== form.experience && (
              <button type="button" className="btn btn-ghost btn-small" onClick={() => set("experience", experiencePrev)}>
                {t("tidy.restore")}
              </button>
            )}
          </div>
        )}

        <Section title={t("profile.sectionWhere")} side={hard("location", "profile.sectionWhere")}>
          <Chips
            legend={t("profile.workModes")}
            values={MODES}
            chosen={form.workModes}
            label={(m) => t(`remote.${m}`)}
            onToggle={(m) => set("workModes", toggle(form.workModes, m))}
          />

          <fieldset className="crit-group">
            <legend>
              {t("profile.cities")} <Help text={t("profile.citiesHelp")} />
            </legend>
            {form.cities.map((city, i) => (
              <div key={i} className="city-row">
                <label className="field">
                  <span className="sr-only">{t("profile.cityName")}</span>
                  <input
                    maxLength={100}
                    placeholder={t("profile.cityName")}
                    value={city.name}
                    onChange={(e) => set("cities", form.cities.map((c, j) => (j === i ? { ...c, name: e.target.value } : c)))}
                  />
                </label>
                <label className="field">
                  <span className="sr-only">{t("profile.radius")}</span>
                  <select
                    value={city.radiusKm}
                    onChange={(e) =>
                      set("cities", form.cities.map((c, j) => (j === i ? { ...c, radiusKm: Number(e.target.value) } : c)))
                    }
                  >
                    {RADII.map((r) => (
                      <option key={r} value={r}>
                        {r === 0 ? t("profile.radius0") : t("profile.radiusKm", { km: r })}
                      </option>
                    ))}
                  </select>
                </label>
                <button
                  type="button"
                  className="icon-btn"
                  aria-label={t("profile.removeCity", { name: city.name || String(i + 1) })}
                  onClick={() => set("cities", form.cities.filter((_, j) => j !== i))}
                >
                  <IconTrash />
                </button>
              </div>
            ))}
            {form.cities.length < 20 && (
              <button
                type="button"
                className="btn btn-ghost btn-small"
                onClick={() => set("cities", [...form.cities, { name: "", radiusKm: 20 }])}
              >
                <IconPlus /> {t("profile.addCity")}
              </button>
            )}
          </fieldset>

          <fieldset className="crit-group">
            <legend>
              {t("profile.commute")} <Help text={t("profile.commuteHelp")} />
            </legend>
            <div className="commute">
              <label className="choice-item">
                <input type="radio" name="commute" checked={form.commute === null} onChange={() => set("commute", null)} />
                <span>{t("profile.commuteNo")}</span>
              </label>
              <label className="choice-item">
                <input
                  type="radio"
                  name="commute"
                  checked={form.commute !== null}
                  onChange={() => set("commute", form.commute ?? 1)}
                />
                <span>{t("profile.commuteSome")}</span>
              </label>
              <input
                type="number"
                min={1}
                max={31}
                className="commute-count"
                aria-label={`${t("profile.commuteSome")} ... ${t("profile.commuteUnit")}`}
                value={form.commute ?? 1}
                disabled={form.commute === null}
                onChange={(e) => set("commute", Math.max(1, Math.min(31, Math.round(Number(e.target.value) || 1))))}
              />
              <span>{t("profile.commuteUnit")}</span>
            </div>
          </fieldset>
        </Section>

        <Section title={t("profile.sectionContract")}>
          <Chips
            legend={t("profile.contracts")}
            values={CONTRACTS}
            chosen={form.contracts}
            label={(c) => t(`contract.${c}`)}
            onToggle={(c) => set("contracts", toggle(form.contracts, c))}
            side={hard("contract", "profile.contracts")}
          />
          <Chips
            legend={t("profile.levels")}
            values={LEVELS}
            chosen={form.levels}
            label={(l) => t(`level.${l}`)}
            onToggle={(l) => set("levels", toggle(form.levels, l))}
            side={hard("level", "profile.levels")}
          />
          <Chips
            legend={t("profile.languages")}
            values={LANGUAGES}
            chosen={form.languages}
            label={(l) => t(`language.${l}`)}
            onToggle={(l) => set("languages", toggle(form.languages, l))}
            side={hard("language", "profile.languages")}
          />
          <p className="hint">{t("profile.anyHint")}</p>
        </Section>

        <Section
          title={t("profile.sectionSalary")}
          help={t("profile.salaryHelp")}
          side={hard("salary", "profile.sectionSalary")}
        >
          <div className="salary-grid" role="group" aria-label={t("profile.sectionSalary")}>
            <span />
            <span className="salary-head">{t("profile.salaryMin")}</span>
            <span className="salary-head">{t("profile.salaryExpected")}</span>
            <span className="salary-head">{t("profile.basis")}</span>
            <span className="salary-head">{t("profile.period")}</span>
            {(["uop", "b2b"] as const).map((which) => {
              const row = form[which];
              const name = t(which === "uop" ? "profile.salaryUop" : "profile.salaryB2b");
              return (
                <SalaryRow key={which} name={name} row={row} onChange={(patch) => setSalary(which, patch)} />
              );
            })}
          </div>
        </Section>

        <Section title={t("profile.sectionWords")}>
          <ListField
            label={t("profile.keywords")}
            hint={t("profile.listHint")}
            value={form.keywords}
            onChange={(v) => set("keywords", v)}
            side={hard("keywords", "profile.keywords")}
          />
          <ListField
            label={t("profile.excludeWords")}
            hint={`${t("profile.listHint")} ${t("profile.excludeHint")}`}
            value={form.excludeWords}
            onChange={(v) => set("excludeWords", v)}
            side={hard("excludeWords", "profile.excludeWords")}
          />
          <ListField
            label={t("profile.skipCompanies")}
            hint={t("profile.listHint")}
            value={form.skipCompanies}
            onChange={(v) => set("skipCompanies", v)}
            side={hard("companies", "profile.skipCompanies")}
          />
        </Section>

        <footer className="sheet-foot">
          {profile && (
            <button type="button" className="btn btn-danger sheet-foot-start" onClick={() => onDelete(profile)}>
              <IconTrash /> {t("profile.delete")}
            </button>
          )}
          <button type="button" className="btn btn-ghost" onClick={onClose}>
            {t("add.cancel")}
          </button>
          <button type="submit" className="btn btn-sign" disabled={busy}>
            {t("profile.save")}
          </button>
        </footer>
      </form>
      {tidy && (
        <TidyDialog
          open
          before={tidy.before}
          proposal={tidy.proposal}
          onClose={() => setTidy(null)}
          onAccept={(text) => {
            set(tidy.field, text);
            setTidy(null);
          }}
        />
      )}
    </Modal>
  );
}

function Section({ title, help, side, children }: { title: string; help?: string; side?: ReactNode; children: ReactNode }) {
  return (
    <section className="crit">
      <header className="crit-head">
        <h3>
          {title} {help && <Help text={help} />}
        </h3>
        {side}
      </header>
      {children}
    </section>
  );
}

/** "Hard filter / only in AI score" for one criterion. */
function HardSwitch({ value, label, onChange }: { value: boolean; label: string; onChange: (value: boolean) => void }) {
  const { t } = useI18n();
  return (
    <span className="hard-switch" role="group" aria-label={label}>
      <button type="button" className="seg seg-small" aria-pressed={value} onClick={() => onChange(true)}>
        {t("profile.hard")}
      </button>
      <button type="button" className="seg seg-small" aria-pressed={!value} onClick={() => onChange(false)}>
        {t("profile.soft")}
      </button>
      <Help text={t("profile.hardHelp")} />
    </span>
  );
}

function Chips<T extends string>({
  legend,
  values,
  chosen,
  label,
  onToggle,
  side,
}: {
  legend: string;
  values: T[];
  chosen: T[];
  label: (value: T) => string;
  onToggle: (value: T) => void;
  side?: ReactNode;
}) {
  return (
    <fieldset className="crit-group crit-row">
      <legend>{legend}</legend>
      <div className="chips">
        {values.map((v) => (
          <label key={v} className="chip">
            <input type="checkbox" checked={chosen.includes(v)} onChange={() => onToggle(v)} />
            <span>{label(v)}</span>
          </label>
        ))}
      </div>
      {side}
    </fieldset>
  );
}

function SalaryRow({ name, row, onChange }: { name: string; row: SalaryForm; onChange: (patch: Partial<SalaryForm>) => void }) {
  const { t } = useI18n();
  return (
    <>
      <span className="salary-name">{name}</span>
      <input
        inputMode="numeric"
        aria-label={`${name}: ${t("profile.salaryMin")}`}
        value={row.min}
        onChange={(e) => onChange({ min: e.target.value })}
      />
      <input
        inputMode="numeric"
        aria-label={`${name}: ${t("profile.salaryExpected")}`}
        value={row.expected}
        onChange={(e) => onChange({ expected: e.target.value })}
      />
      <select aria-label={`${name}: ${t("profile.basis")}`} value={row.basis} onChange={(e) => onChange({ basis: e.target.value as PayBasis })}>
        <option value="gross">{t("basis.gross")}</option>
        <option value="net">{t("basis.net")}</option>
      </select>
      <select
        aria-label={`${name}: ${t("profile.period")}`}
        value={row.period}
        onChange={(e) => onChange({ period: e.target.value as SalaryPeriod })}
      >
        <option value="month">{t("add.periodMonth")}</option>
        <option value="hour">{t("add.periodHour")}</option>
      </select>
    </>
  );
}

function ListField({
  label,
  hint,
  value,
  onChange,
  side,
}: {
  label: string;
  hint: string;
  value: string;
  onChange: (value: string) => void;
  side?: ReactNode;
}) {
  return (
    <div className="crit-row">
      <label className="field">
        <span>{label}</span>
        <input maxLength={5000} value={value} onChange={(e) => onChange(e.target.value)} />
        <span className="hint">{hint}</span>
      </label>
      {side}
    </div>
  );
}
