import type { Profile } from "../api/types";
import { useI18n } from "../i18n";
import { IconArrowRight, IconPencil, IconPlus } from "./Icons";

/** Profiles as direction signs on a gantry above the board: pick one, or see all together. */
export function ProfileTabs({
  profiles,
  current,
  onPick,
  onEdit,
  onAdd,
  disabled,
}: {
  profiles: Profile[];
  current: number | null;
  onPick: (id: number | null) => void;
  onEdit: (profile: Profile) => void;
  onAdd: () => void;
  disabled?: boolean;
}) {
  const { t } = useI18n();
  return (
    <nav className="gantry" aria-label={t("profiles.label")}>
      <ul className="signs">
        {profiles.length > 1 && (
          <li>
            <button
              type="button"
              className="sign"
              aria-current={current === null ? "true" : undefined}
              onClick={() => onPick(null)}
            >
              {t("profiles.all")}
            </button>
          </li>
        )}
        {profiles.map((p) => (
          <li key={p.id} className="sign-pair">
            <button
              type="button"
              className="sign"
              aria-current={current === p.id || profiles.length === 1 ? "true" : undefined}
              onClick={() => onPick(p.id)}
            >
              <IconArrowRight /> {p.name}
            </button>
            <button
              type="button"
              className="sign sign-edit"
              aria-label={t("profiles.edit", { name: p.name })}
              title={t("profiles.edit", { name: p.name })}
              onClick={() => onEdit(p)}
              disabled={disabled}
            >
              <IconPencil />
            </button>
          </li>
        ))}
        <li>
          <button type="button" className="sign sign-add" onClick={onAdd} disabled={disabled}>
            <IconPlus /> {t("profiles.add")}
          </button>
        </li>
      </ul>
      {profiles.length === 0 && <p className="gantry-hint">{t("profiles.none")}</p>}
    </nav>
  );
}
