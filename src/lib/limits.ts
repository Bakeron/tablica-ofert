/** Numbers shared with Rust - `limits_match_the_interface` in settings.rs fails when they differ. */

/** Calls a Jooble key allows in its whole life (`jooble::KEY_CALLS`). */
export const JOOBLE_KEY_CALLS = 500;
/** Days after "applied" without a later stage before a follow-up (`agenda::FOLLOW_UP_DAYS`). */
export const FOLLOW_UP_DAYS = 7;
/** Below this word match an offer goes to "To check" (`profiles::LOW_KEYWORD_SCORE`). */
export const LOW_KEYWORD_SCORE = 30;
/** Pay this many times above the expectation goes to "To check" (`filter::FAR_ABOVE`). */
export const SALARY_FAR_ABOVE = 2;
/** Shortest password of a backup copy (`backup::MIN_PASSWORD`). */
export const PASSWORD_MIN = 8;
/** Longest password of a backup copy (`backup::MAX_PASSWORD`). */
export const PASSWORD_MAX = 200;
/** Longest key or password (`secrets::MAX_SECRET`). */
export const SECRET_MAX = 512;
/** Ratings needed on each side ("Very" and "No") before anything is learned (`learn::MIN_RATINGS`). */
export const LEARN_MIN_RATINGS = 2;
/** The largest correction of a score, up or down (`learn::MAX_POINTS`). */
export const LEARN_MAX_POINTS = 15;
/** Longest short field of an offer - title, company, city, contract, level (`offers::MAX_SHORT`). */
export const OFFER_SHORT_MAX = 300;
/** Longest link of an offer (`offers::MAX_URL`). */
export const OFFER_URL_MAX = 2000;
/** Longest description or pasted text of an offer (`offers::MAX_LONG`). */
export const OFFER_LONG_MAX = 100000;
/** Longest note at an offer (`offers::MAX_NOTES`). */
export const OFFER_NOTES_MAX = 20000;
/** Longest profile name (`profiles::MAX_NAME`). */
export const PROFILE_NAME_MAX = 40;
/** Longest "What I am looking for" (`profiles::MAX_DESCRIPTION`). */
export const PROFILE_DESCRIPTION_MAX = 10000;
/** Longest work experience (`profiles::MAX_EXPERIENCE`). */
export const PROFILE_EXPERIENCE_MAX = 20000;
/** Longest single word or city of a profile (`profiles::MAX_WORD`). */
export const PROFILE_WORD_MAX = 100;
/** Longest note of a date or a contact log entry (`agenda::MAX_NOTE`). */
export const AGENDA_NOTE_MAX = 2000;
/** Longest field of a contact (`agenda::MAX_FIELD`). */
export const AGENDA_FIELD_MAX = 200;
/** Longest value of a text setting (`max` of the rules in settings.rs). */
export const TEXT_MAX = {
  google_client_id: 200,
  mail_host: 253,
  mail_port: 5,
  mail_user: 254,
  search_words: 254,
  ai_model: 120,
  ai_daily_limit: 5,
  ai_topup_usd: 8,
  ai_day_usd: 8,
  ai_topup_date: 10,
  ollama_url: 200,
};
