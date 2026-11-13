//! Claude's spending counter: tokens of every paid call (per day and model, table `ai_spend`)
//! and their cost from the price list below. A normal API key cannot read the account's
//! balance, so the app counts what it spent itself. Free providers are not counted here.

use rusqlite::{Connection, params};
use serde::Serialize;
use serde_json::Value;

use crate::error::AppResult;

/// Token counts from one answer (`usage` of the Anthropic Messages API).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tokens {
    /// Full-price input (the part of the prompt that was not in the cache).
    pub input: i64,
    pub output: i64,
    /// Read back from the prompt cache.
    pub cache_read: i64,
    /// Written to the prompt cache (5 minutes).
    pub cache_write: i64,
}

impl Tokens {
    /// `None` when the answer has no token counts at all. Negative or broken numbers count as 0.
    pub fn from_usage(usage: &Value) -> Option<Self> {
        let n = |field: &str| usage.get(field).and_then(Value::as_i64).map(|v| v.max(0));
        let fields = ["input_tokens", "output_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"].map(n);
        if fields.iter().all(Option::is_none) {
            return None;
        }
        let [input, output, cache_read, cache_write] = fields.map(Option::unwrap_or_default);
        Some(Tokens { input, output, cache_read, cache_write })
    }
}

/// Adds one call's tokens to today's row of this model (today in the local time zone).
pub fn record(conn: &Connection, model: &str, t: &Tokens) -> AppResult<()> {
    let day = crate::clock::local_day();
    conn.execute(
        "INSERT INTO ai_spend (day, model, calls, input, output, cache_read, cache_write) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6)
         ON CONFLICT (day, model) DO UPDATE SET calls = calls + 1, input = input + excluded.input,
             output = output + excluded.output, cache_read = cache_read + excluded.cache_read,
             cache_write = cache_write + excluded.cache_write",
        params![day, crate::text::one_line(model, 120), t.input, t.output, t.cache_read, t.cache_write],
    )?;
    Ok(())
}

/// Dollars per million tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    /// 5-minute cache write = 1.25 x input.
    pub cache_write: f64,
}

const fn price(input: f64, output: f64, cache_read: f64) -> Price {
    Price { input, output, cache_read, cache_write: input * 1.25 }
}

/// Claude's price list (Anthropic's pricing page, as of 3.11.2026). The answer may name a model
/// with a date at the end, so a model matches by the beginning of its name. Haiku 5.5: prompts up
/// to 100 thousand tokens (offers are far shorter).
const PRICES: &[(&str, Price)] = &[
    ("claude-sonnet-5-5", price(2.0, 10.0, 0.10)),
    ("claude-opus-5-5", price(4.0, 20.0, 0.20)),
    ("claude-haiku-5-5", price(0.10, 0.50, 0.01)),
    ("claude-fable-5-2", price(8.0, 40.0, 0.20)),
    // Older models a safety fallback ("fallbacks": "default") may answer with.
    ("claude-sonnet-5", price(2.0, 10.0, 0.20)),
    ("claude-opus-5", price(5.0, 25.0, 0.50)),
    ("claude-opus-4-8", price(5.0, 25.0, 0.50)),
];

/// A model missing from the list is counted at Opus 5.5's price: the counter rather shows too
/// much than too little.
const UNKNOWN: Price = price(4.0, 20.0, 0.20);

/// The price of a model. The longest matching name wins (`claude-sonnet-5-5` before `claude-sonnet-5`).
pub fn price_of(model: &str) -> Price {
    let model = model.trim().to_ascii_lowercase();
    PRICES
        .iter()
        .filter(|(name, _)| model == *name || model.strip_prefix(name).is_some_and(|rest| rest.starts_with(['-', '@'])))
        .max_by_key(|(name, _)| name.len())
        .map_or(UNKNOWN, |(_, p)| *p)
}

/// The cost of tokens in dollars.
pub fn cost_usd(model: &str, t: &Tokens) -> f64 {
    let p = price_of(model);
    (t.input as f64 * p.input + t.output as f64 * p.output + t.cache_read as f64 * p.cache_read + t.cache_write as f64 * p.cache_write)
        / 1_000_000.0
}

/// Claude's spending in dollars for the settings: today, this calendar month and since the
/// day of the last top-up (`None` when no day is given). Days are local, as `record` keeps them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spent {
    pub today: f64,
    pub month: f64,
    pub since_top_up: Option<f64>,
}

pub fn spent(conn: &Connection, top_up_day: &str) -> AppResult<Spent> {
    spent_on(conn, &crate::clock::local_day(), top_up_day)
}

fn spent_on(conn: &Connection, today: &str, top_up_day: &str) -> AppResult<Spent> {
    let month_start = format!("{}-01", today.get(..7).unwrap_or_default());
    let top_up_day = (top_up_day.len() == 10).then_some(top_up_day);
    let mut out = Spent { since_top_up: top_up_day.map(|_| 0.0), ..Default::default() };
    // One row per day and model - small enough to add up here, with each model's own price.
    let mut stmt = conn.prepare("SELECT day, model, input, output, cache_read, cache_write FROM ai_spend")?;
    let rows = stmt.query_map([], |r| {
        let t = Tokens { input: r.get(2)?, output: r.get(3)?, cache_read: r.get(4)?, cache_write: r.get(5)? };
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, t))
    })?;
    for row in rows {
        let (day, model, t) = row?;
        let usd = cost_usd(&model, &t);
        if day == today {
            out.today += usd;
        }
        if day.as_str() >= month_start.as_str() && day.as_str() <= today {
            out.month += usd;
        }
        if let (Some(from), Some(sum)) = (top_up_day, out.since_top_up.as_mut())
            && day.as_str() >= from
        {
            *sum += usd;
        }
    }
    Ok(out)
}

/// Below this share of the top-up left the app warns (in the settings and after a rating).
const LOW_SHARE: f64 = 0.10;

/// What is left of the last top-up, in $ - only when it is getting low.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LowFunds {
    pub left: f64,
    pub top_up: f64,
}

/// `Some` when a top-up with its day is given and less than 10% of it is left (spent more than
/// the top-up = 0 left). `top_up` as the settings keep it ("5", "20.50", empty = not given).
pub fn low(top_up: &str, since_top_up: Option<f64>) -> Option<LowFunds> {
    let top_up: f64 = top_up.parse().ok().filter(|v: &f64| v.is_finite() && *v > 0.0)?;
    let left = (top_up - since_top_up?).max(0.0);
    (left < top_up * LOW_SHARE).then_some(LowFunds { left, top_up })
}

/// The same from the database - after a call to Claude.
pub fn low_funds(conn: &Connection) -> AppResult<Option<LowFunds>> {
    let day = crate::settings::get(conn, "ai_topup_date")?;
    Ok(low(&crate::settings::get(conn, "ai_topup_usd")?, spent(conn, &day)?.since_top_up))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use serde_json::json;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn reads_the_token_counts_of_an_answer() {
        let usage = json!({
            "input_tokens": 512, "output_tokens": 830,
            "cache_read_input_tokens": 1700, "cache_creation_input_tokens": 0,
            "cache_creation": { "ephemeral_5m_input_tokens": 0 }
        });
        assert_eq!(Tokens::from_usage(&usage), Some(Tokens { input: 512, output: 830, cache_read: 1700, cache_write: 0 }));
        // Fields missing (an older answer) or broken: 0, never a negative cost.
        assert_eq!(Tokens::from_usage(&json!({ "input_tokens": 10, "output_tokens": -5 })), Some(Tokens { input: 10, ..Default::default() }));
        assert_eq!(Tokens::from_usage(&json!({})), None);
        assert_eq!(Tokens::from_usage(&json!({ "input_tokens": "a lot" })), None);
    }

    #[test]
    fn adds_calls_up_per_day_and_model() {
        let conn = test_db();
        record(&conn, "claude-sonnet-5-5", &Tokens { input: 100, output: 50, cache_read: 1000, cache_write: 0 }).unwrap();
        record(&conn, "claude-sonnet-5-5", &Tokens { input: 10, output: 5, cache_read: 0, cache_write: 1700 }).unwrap();
        record(&conn, "claude-sonnet-5", &Tokens { input: 1, output: 1, cache_read: 0, cache_write: 0 }).unwrap();
        let row: (i64, i64, i64, i64, i64) = conn
            .query_row(
                "SELECT calls, input, output, cache_read, cache_write FROM ai_spend WHERE model = 'claude-sonnet-5-5'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(row, (2, 110, 55, 1000, 1700));
        let rows: i64 = conn.query_row("SELECT COUNT(*) FROM ai_spend", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 2);
    }

    #[test]
    fn a_model_gets_its_own_price() {
        assert_eq!(price_of("claude-sonnet-5-5"), price(2.0, 10.0, 0.10));
        // With a date at the end, other letter case.
        assert_eq!(price_of("Claude-Sonnet-5-5-20261001"), price(2.0, 10.0, 0.10));
        // The older Sonnet 5 is not mistaken for Sonnet 5.5 and the other way round.
        assert_eq!(price_of("claude-sonnet-5"), price(2.0, 10.0, 0.20));
        assert_eq!(price_of("claude-opus-5"), price(5.0, 25.0, 0.50));
        assert_eq!(price_of("claude-opus-5-5"), price(4.0, 20.0, 0.20));
        // Unknown: Opus 5.5's price, a name that only starts the same is not a match.
        assert_eq!(price_of("claude-sonnet-55"), UNKNOWN);
        assert_eq!(price_of(""), UNKNOWN);
        assert!(near(price_of("claude-sonnet-5-5").cache_write, 2.5));
    }

    /// Numbers from the test of ratings (11.11.2026, `ai/eval.rs`): one rating with Sonnet 5.5
    /// at low effort, the prompt read from the cache - about $0.008.
    #[test]
    fn the_cost_of_a_call_comes_from_its_tokens() {
        let rating = Tokens { input: 520, output: 600, cache_read: 1700, cache_write: 0 };
        // 520 x $2 + 600 x $10 + 1700 x $0.10 per million.
        assert!(near(cost_usd("claude-sonnet-5-5", &rating), 0.00721));
        // The first offer of a series writes the prompt to the cache (1.25 x input).
        let first = Tokens { input: 520, output: 600, cache_read: 0, cache_write: 1700 };
        assert!(near(cost_usd("claude-sonnet-5-5", &first), 0.01129));
        // The same tokens on Opus 5.5 cost twice as much.
        assert!(near(cost_usd("claude-opus-5-5", &rating), 2.0 * 0.00721));
        assert!(near(cost_usd("claude-sonnet-5-5", &Tokens::default()), 0.0));
    }

    /// Today, this month and since the top-up - each model at its own price; a top-up day not
    /// given = nothing counted since it.
    #[test]
    fn adds_up_the_spending_for_the_settings() {
        let conn = test_db();
        let add = |day: &str, model: &str, output: i64| {
            conn.execute(
                "INSERT INTO ai_spend (day, model, calls, output) VALUES (?1, ?2, 1, ?3)",
                params![day, model, output],
            )
            .unwrap();
        };
        // $10 and $20 per million output tokens: 100 000 tokens = $1 on Sonnet, $2 on Opus.
        add("2026-10-30", "claude-sonnet-5-5", 100_000);
        add("2026-11-02", "claude-sonnet-5-5", 100_000);
        add("2026-11-11", "claude-sonnet-5-5", 50_000);
        add("2026-11-11", "claude-opus-5-5", 100_000);
        let s = spent_on(&conn, "2026-11-11", "2026-10-30").unwrap();
        assert!(near(s.today, 2.5) && near(s.month, 3.5), "{s:?}");
        assert!(near(s.since_top_up.unwrap(), 4.5));
        assert!(near(spent_on(&conn, "2026-11-11", "2026-11-03").unwrap().since_top_up.unwrap(), 2.5));
        assert_eq!(spent_on(&conn, "2026-11-11", "").unwrap().since_top_up, None);
        // A new month starts from zero, a top-up made today counts today.
        let s = spent_on(&conn, "2026-12-01", "2026-12-01").unwrap();
        assert_eq!((s.today, s.month, s.since_top_up), (0.0, 0.0, Some(0.0)));
    }

    #[test]
    fn warns_when_less_than_a_tenth_of_the_top_up_is_left() {
        assert_eq!(low("5", Some(4.0)), None);
        assert_eq!(low("5", Some(4.5)), None);
        let l = low("5", Some(4.6)).unwrap();
        assert!(near(l.left, 0.4) && near(l.top_up, 5.0));
        // Spent more than the top-up (the counter is an estimate): 0 left, never below.
        assert_eq!(low("5", Some(7.0)), Some(LowFunds { left: 0.0, top_up: 5.0 }));
        // No top-up or no day: nothing to compare with.
        assert_eq!(low("", Some(100.0)), None);
        assert_eq!(low("5", None), None);
        assert_eq!(low("0", Some(1.0)), None);
    }

    #[test]
    fn reads_the_top_up_from_the_settings() {
        let conn = test_db();
        let day = crate::clock::now()[..10].to_string();
        conn.execute("INSERT INTO ai_spend (day, model, calls, output) VALUES (?1, 'claude-sonnet-5-5', 1, 460000)", [&day]).unwrap();
        assert_eq!(low_funds(&conn).unwrap(), None);
        crate::settings::set(&conn, "ai_topup_usd", "5").unwrap();
        assert_eq!(low_funds(&conn).unwrap(), None, "a sum without a day is not compared");
        crate::settings::set(&conn, "ai_topup_date", &day).unwrap();
        // 460 000 output tokens on Sonnet = $4.60 of $5.
        let l = low_funds(&conn).unwrap().unwrap();
        assert!(near(l.left, 0.4), "{l:?}");
    }
}
