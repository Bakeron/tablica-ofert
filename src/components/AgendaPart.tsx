import { useRef, useState, type FormEvent } from "react";
import type { Agenda, Contact, EventKind, LogKind } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { fullDate, isWebUrl, localInputToUtc, nowLocalInput } from "../lib/format";
import { isPhone } from "../lib/platform";
import { IconPlus, IconTrash } from "./Icons";
import { Reveal } from "./Reveal";
import { AGENDA_FIELD_MAX, AGENDA_NOTE_MAX } from "../lib/limits";

const EVENT_KINDS: EventKind[] = ["interview", "deadline", "other"];
const LOG_KINDS: LogKind[] = ["call", "mail", "meeting", "message"];
const EMPTY_CONTACT: Contact = { id: 0, name: "", role: "", email: "", phone: "", linkedin: "" };

export interface AgendaActions {
  addEvent: (event: { kind: EventKind; at: string; minutes: number; note: string }) => Promise<boolean>;
  deleteEvent: (id: number) => void;
  google: (id: number) => void;
  ics: (id: number) => void;
  saveContact: (contact: Contact) => Promise<boolean>;
  deleteContact: (id: number) => void;
  addLog: (entry: { at: string; kind: LogKind; note: string }) => Promise<boolean>;
  deleteLog: (id: number) => void;
  openLink: (url: string) => void;
}

/** Dates (with "Add to calendar"), contacts and contact history of one offer. */
export function AgendaPart({
  agenda,
  actions,
  showDates,
  showContacts,
}: {
  agenda: Agenda;
  actions: AgendaActions;
  showDates: boolean;
  showContacts: boolean;
}) {
  const { t, lang } = useI18n();
  // Minutes may be empty for a moment while a new number is typed ("" - the field asks for one).
  const [eventForm, setEventForm] = useState<{ kind: EventKind; at: string; minutes: number | ""; note: string } | null>(null);
  const [contactForm, setContactForm] = useState<Contact | null>(null);
  const [logForm, setLogForm] = useState<{ at: string; kind: LogKind; note: string } | null>(null);

  // A double click on "Save" saves once - the form closes only after the first save is done.
  const saving = useRef(false);
  async function once(work: () => Promise<void>) {
    if (saving.current) return;
    saving.current = true;
    try {
      await work();
    } finally {
      saving.current = false;
    }
  }

  function submitEvent(e: FormEvent) {
    e.preventDefault();
    return once(async () => {
      if (!eventForm) return;
      const at = localInputToUtc(eventForm.at);
      if (!at) return;
      if (await actions.addEvent({ ...eventForm, at, minutes: Number(eventForm.minutes) || 60 })) setEventForm(null);
    });
  }

  function submitContact(e: FormEvent) {
    e.preventDefault();
    return once(async () => {
      if (contactForm && (await actions.saveContact(contactForm))) setContactForm(null);
    });
  }

  function submitLog(e: FormEvent) {
    e.preventDefault();
    return once(async () => {
      if (!logForm) return;
      const at = localInputToUtc(logForm.at);
      if (at && (await actions.addLog({ ...logForm, at }))) setLogForm(null);
    });
  }

  return (
    <>
      {showDates && (
      <section className="pass-text agenda" aria-labelledby="dates-title">
        <h3 id="dates-title" className="pass-label">
          {t("agenda.dates")}
        </h3>
        {agenda.events.length === 0 && !eventForm && <p className="pass-meta">{t("agenda.noDates")}</p>}
        <ul className="agenda-list">
          {agenda.events.map((ev) => (
            <li key={ev.id}>
              <span className="agenda-when mono">{fullDate(ev.at, lang)}</span>
              <span className="agenda-what">
                <strong>{t(`event.${ev.kind}` as TextKey)}</strong>
                {ev.note && ` - ${ev.note}`}
              </span>
              <span className="agenda-tools">
                <button type="button" className="btn btn-ghost btn-small" onClick={() => actions.google(ev.id)}>
                  {t("agenda.google")}
                </button>
                {/* Android keeps the saved file in the app's private folder, so on the phone only Google Calendar */}
                {!isPhone() && (
                  <button type="button" className="btn btn-ghost btn-small" onClick={() => actions.ics(ev.id)} title={t("agenda.icsHint")}>
                    {t("agenda.ics")}
                  </button>
                )}
                <button type="button" className="icon-btn icon-btn-small" aria-label={t("agenda.delete")} title={t("agenda.delete")} onClick={() => actions.deleteEvent(ev.id)}>
                  <IconTrash />
                </button>
              </span>
            </li>
          ))}
        </ul>
        <Reveal show={eventForm !== null}>
          {eventForm && (
          <form className="agenda-form" onSubmit={submitEvent}>
            <label className="field">
              <span>{t("agenda.kind")}</span>
              <select value={eventForm.kind} onChange={(e) => setEventForm({ ...eventForm, kind: e.target.value as EventKind })}>
                {EVENT_KINDS.map((k) => (
                  <option key={k} value={k}>
                    {t(`event.${k}` as TextKey)}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>{t("agenda.when")}</span>
              <input type="datetime-local" required value={eventForm.at} onChange={(e) => setEventForm({ ...eventForm, at: e.target.value })} />
            </label>
            <label className="field">
              <span>{t("agenda.minutes")}</span>
              <input type="number" min={5} max={1440} step={5} required value={eventForm.minutes} onChange={(e) => setEventForm({ ...eventForm, minutes: e.target.value === "" ? "" : Number(e.target.value) })} />
            </label>
            <label className="field agenda-note">
              <span>{t("agenda.note")}</span>
              <input maxLength={AGENDA_NOTE_MAX} value={eventForm.note} onChange={(e) => setEventForm({ ...eventForm, note: e.target.value })} />
            </label>
            <div className="agenda-buttons">
              <button type="button" className="btn btn-ghost btn-small" onClick={() => setEventForm(null)}>
                {t("add.cancel")}
              </button>
              <button type="submit" className="btn btn-sign btn-small">
                {t("agenda.save")}
              </button>
            </div>
            <p className="hint">{t(isPhone() ? "agenda.remindHintPhone" : "agenda.remindHint")}</p>
          </form>
          )}
        </Reveal>
        {!eventForm && (
          <button type="button" className="btn btn-ghost btn-small" onClick={() => setEventForm({ kind: "interview", at: nowLocalInput(), minutes: 60, note: "" })}>
            <IconPlus /> {t("agenda.addDate")}
          </button>
        )}
      </section>
      )}

      {showContacts && (
      <section className="pass-text agenda" aria-labelledby="contacts-title">
        <h3 id="contacts-title" className="pass-label">
          {t("agenda.contacts")}
        </h3>
        <ul className="agenda-list">
          {agenda.contacts.map((c) => (
            <li key={c.id}>
              <span className="agenda-what">
                <strong>{c.name}</strong>
                {c.role && ` - ${c.role}`}
                {c.email && <span className="agenda-sub mono">{c.email}</span>}
                {c.phone && <span className="agenda-sub mono">{c.phone}</span>}
                {isWebUrl(c.linkedin) && (
                  <a
                    href={c.linkedin}
                    className="agenda-sub"
                    onClick={(e) => {
                      e.preventDefault();
                      actions.openLink(c.linkedin);
                    }}
                  >
                    LinkedIn
                  </a>
                )}
              </span>
              <span className="agenda-tools">
                <button type="button" className="btn btn-ghost btn-small" onClick={() => setContactForm({ ...c })}>
                  {t("agenda.edit")}
                </button>
                <button type="button" className="icon-btn icon-btn-small" aria-label={t("agenda.delete")} title={t("agenda.delete")} onClick={() => actions.deleteContact(c.id)}>
                  <IconTrash />
                </button>
              </span>
            </li>
          ))}
        </ul>
        <Reveal show={contactForm !== null}>
          {contactForm && (
          <form className="agenda-form" onSubmit={submitContact} autoComplete="off">
            {(["name", "role", "email", "phone", "linkedin"] as const).map((key) => (
              <label key={key} className="field">
                <span>
                  {t(`contact.${key}` as TextKey)}
                  {key === "name" && <em className="req"> ({t("add.required")})</em>}
                </span>
                <input
                  required={key === "name"}
                  type={key === "email" ? "email" : key === "linkedin" ? "url" : key === "phone" ? "tel" : "text"}
                  maxLength={AGENDA_FIELD_MAX}
                  value={contactForm[key]}
                  onChange={(e) => setContactForm({ ...contactForm, [key]: e.target.value })}
                />
              </label>
            ))}
            <div className="agenda-buttons">
              <button type="button" className="btn btn-ghost btn-small" onClick={() => setContactForm(null)}>
                {t("add.cancel")}
              </button>
              <button type="submit" className="btn btn-sign btn-small">
                {t("agenda.save")}
              </button>
            </div>
          </form>
          )}
        </Reveal>
        {!contactForm && (
          <button type="button" className="btn btn-ghost btn-small" onClick={() => setContactForm({ ...EMPTY_CONTACT })}>
            <IconPlus /> {t("agenda.addContact")}
          </button>
        )}

        <h4 className="pass-label agenda-sub-title">{t("agenda.log")}</h4>
        <ul className="agenda-list">
          {agenda.log.map((l) => (
            <li key={l.id}>
              <span className="agenda-when mono">{fullDate(l.at, lang)}</span>
              <span className="agenda-what">
                <strong>{t(`log.${l.kind}` as TextKey)}</strong>
                {l.note && ` - ${l.note}`}
              </span>
              <span className="agenda-tools">
                <button type="button" className="icon-btn icon-btn-small" aria-label={t("agenda.delete")} title={t("agenda.delete")} onClick={() => actions.deleteLog(l.id)}>
                  <IconTrash />
                </button>
              </span>
            </li>
          ))}
        </ul>
        <Reveal show={logForm !== null}>
          {logForm && (
          <form className="agenda-form" onSubmit={submitLog}>
            <label className="field">
              <span>{t("agenda.kind")}</span>
              <select value={logForm.kind} onChange={(e) => setLogForm({ ...logForm, kind: e.target.value as LogKind })}>
                {LOG_KINDS.map((k) => (
                  <option key={k} value={k}>
                    {t(`log.${k}` as TextKey)}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>{t("agenda.when")}</span>
              <input type="datetime-local" required value={logForm.at} onChange={(e) => setLogForm({ ...logForm, at: e.target.value })} />
            </label>
            <label className="field agenda-note">
              <span>{t("agenda.note")}</span>
              <input maxLength={AGENDA_NOTE_MAX} value={logForm.note} onChange={(e) => setLogForm({ ...logForm, note: e.target.value })} />
            </label>
            <div className="agenda-buttons">
              <button type="button" className="btn btn-ghost btn-small" onClick={() => setLogForm(null)}>
                {t("add.cancel")}
              </button>
              <button type="submit" className="btn btn-sign btn-small">
                {t("agenda.save")}
              </button>
            </div>
          </form>
          )}
        </Reveal>
        {!logForm && (
          <button type="button" className="btn btn-ghost btn-small" onClick={() => setLogForm({ at: nowLocalInput(), kind: "call", note: "" })}>
            <IconPlus /> {t("agenda.addLog")}
          </button>
        )}
      </section>
      )}
    </>
  );
}
