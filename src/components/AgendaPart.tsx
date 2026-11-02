import { useState, type FormEvent } from "react";
import type { Agenda, Contact, EventKind, LogKind } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { fullDate, isWebUrl, localInputToUtc, nowLocalInput } from "../lib/format";
import { IconPlus, IconTrash } from "./Icons";

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
  const [eventForm, setEventForm] = useState<{ kind: EventKind; at: string; minutes: number; note: string } | null>(null);
  const [contactForm, setContactForm] = useState<Contact | null>(null);
  const [logForm, setLogForm] = useState<{ at: string; kind: LogKind; note: string } | null>(null);

  async function submitEvent(e: FormEvent) {
    e.preventDefault();
    if (!eventForm) return;
    const at = localInputToUtc(eventForm.at);
    if (!at) return;
    if (await actions.addEvent({ ...eventForm, at })) setEventForm(null);
  }

  async function submitContact(e: FormEvent) {
    e.preventDefault();
    if (contactForm && (await actions.saveContact(contactForm))) setContactForm(null);
  }

  async function submitLog(e: FormEvent) {
    e.preventDefault();
    if (!logForm) return;
    const at = localInputToUtc(logForm.at);
    if (at && (await actions.addLog({ ...logForm, at }))) setLogForm(null);
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
                <button type="button" className="btn btn-ghost btn-small" onClick={() => actions.ics(ev.id)} title={t("agenda.icsHint")}>
                  {t("agenda.ics")}
                </button>
                <button type="button" className="icon-btn icon-btn-small" aria-label={t("agenda.delete")} title={t("agenda.delete")} onClick={() => actions.deleteEvent(ev.id)}>
                  <IconTrash />
                </button>
              </span>
            </li>
          ))}
        </ul>
        {eventForm ? (
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
              <input type="number" min={5} max={1440} step={5} value={eventForm.minutes} onChange={(e) => setEventForm({ ...eventForm, minutes: Number(e.target.value) || 60 })} />
            </label>
            <label className="field agenda-note">
              <span>{t("agenda.note")}</span>
              <input maxLength={2000} value={eventForm.note} onChange={(e) => setEventForm({ ...eventForm, note: e.target.value })} />
            </label>
            <div className="agenda-buttons">
              <button type="button" className="btn btn-ghost btn-small" onClick={() => setEventForm(null)}>
                {t("add.cancel")}
              </button>
              <button type="submit" className="btn btn-sign btn-small">
                {t("agenda.save")}
              </button>
            </div>
            <p className="hint">{t("agenda.remindHint")}</p>
          </form>
        ) : (
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
        {contactForm ? (
          <form className="agenda-form" onSubmit={submitContact}>
            {(["name", "role", "email", "phone", "linkedin"] as const).map((key) => (
              <label key={key} className="field">
                <span>
                  {t(`contact.${key}` as TextKey)}
                  {key === "name" && <em className="req"> ({t("add.required")})</em>}
                </span>
                <input
                  required={key === "name"}
                  type={key === "email" ? "email" : key === "linkedin" ? "url" : "text"}
                  maxLength={200}
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
        ) : (
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
        {logForm ? (
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
              <input maxLength={2000} value={logForm.note} onChange={(e) => setLogForm({ ...logForm, note: e.target.value })} />
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
        ) : (
          <button type="button" className="btn btn-ghost btn-small" onClick={() => setLogForm({ at: nowLocalInput(), kind: "call", note: "" })}>
            <IconPlus /> {t("agenda.addLog")}
          </button>
        )}
      </section>
      )}
    </>
  );
}
