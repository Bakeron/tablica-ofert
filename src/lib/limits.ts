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
/** Longest key or password (`secrets::MAX_SECRET`). */
export const SECRET_MAX = 512;
/** Ratings needed before anything is learned from them (`learn::MIN_RATINGS`). */
export const LEARN_MIN_RATINGS = 5;
/** The largest correction of a score, up or down (`learn::MAX_POINTS`). */
export const LEARN_MAX_POINTS = 15;
/** Longest value of a text setting (`max` of the rules in settings.rs). */
export const TEXT_MAX = {
  google_client_id: 200,
  mail_host: 253,
  mail_port: 5,
  mail_user: 254,
  search_words: 254,
  ai_model: 120,
  ai_daily_limit: 5,
  ollama_url: 200,
};
