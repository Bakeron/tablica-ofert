//! The dollar's mid rate from the National Bank of Poland (api.nbp.pl, public, free, no key) - only
//! to show Claude's spending in zloty too, roughly. Nothing about the person is sent.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::net;

const URL: &str = "https://api.nbp.pl/api/exchangerates/rates/a/usd/?format=json";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsdRate {
    /// Zloty for one dollar.
    pub pln: f64,
    /// The day of the table the rate comes from (YYYY-MM-DD).
    pub day: String,
}

#[derive(Deserialize)]
struct Answer {
    rates: Vec<Row>,
}

#[derive(Deserialize)]
struct Row {
    #[serde(rename = "effectiveDate")]
    effective_date: String,
    mid: f64,
}

/// The rate asked today - NBP is asked once a day at most.
static TODAY: Mutex<Option<(String, UsdRate)>> = Mutex::new(None);

/// Today's rate, or the last table's (NBP publishes on working days). `None` without a connection -
/// the zloty are then simply not shown.
pub async fn usd_rate() -> Option<UsdRate> {
    let today = crate::clock::now().get(..10).unwrap_or_default().to_string();
    if let Some((day, rate)) = TODAY.lock().ok()?.as_ref()
        && *day == today
    {
        return Some(rate.clone());
    }
    let rate = fetch().await.ok()?;
    *TODAY.lock().ok()? = Some((today, rate.clone()));
    Some(rate)
}

async fn fetch() -> AppResult<UsdRate> {
    let resp = net::client()?.get(URL).send().await.map_err(net::net_error)?;
    read(net::json(resp).await?)
}

fn read(answer: Answer) -> AppResult<UsdRate> {
    let row = answer.rates.into_iter().next().ok_or_else(|| AppError::Net("NBP gave no rate".into()))?;
    let day_ok = row.effective_date.len() == 10 && row.effective_date.bytes().all(|b| b.is_ascii_digit() || b == b'-');
    // A sane rate only (the zloty has been between 1 and 10 for a dollar for decades).
    if !day_ok || !row.mid.is_finite() || !(1.0..=10.0).contains(&row.mid) {
        return Err(AppError::Net("NBP gave a strange rate".into()));
    }
    Ok(UsdRate { pln: row.mid, day: row.effective_date })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sane_rate_is_read_and_a_strange_one_is_not() {
        let answer = |json: &str| serde_json::from_str::<Answer>(json).unwrap();
        let ok = answer(r#"{"table":"A","currency":"dolar amerykański","code":"USD","rates":[{"no":"221/A/NBP/2026","effectiveDate":"2026-11-13","mid":3.6512}]}"#);
        assert_eq!(read(ok).unwrap(), UsdRate { pln: 3.6512, day: "2026-11-13".into() });
        assert!(read(answer(r#"{"rates":[]}"#)).is_err());
        assert!(read(answer(r#"{"rates":[{"effectiveDate":"2026-11-13","mid":365.12}]}"#)).is_err());
        assert!(read(answer(r#"{"rates":[{"effectiveDate":"<b>x</b>","mid":3.6}]}"#)).is_err());
    }
}
