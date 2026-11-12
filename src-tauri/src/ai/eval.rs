//! Test of AI ratings on real offers (zmiany.md, point 4): the same prompt the app sends, the same
//! offers, several providers and Claude's `effort` levels side by side. Not a part of the normal
//! test run (`#[ignore]`) - it reads a COPY of the user's database and costs money.
//!
//! cargo test --lib ai::eval -- --ignored --nocapture --test-threads=1
//! Environment:
//! - `TEST_OCEN_DB` - a copy of `tablica.db` (never the app's own file),
//! - `TEST_OCEN_OUT` - folder for the results (outside the repository: the offers are private),
//! - `TEST_OCEN_IDS` - offer ids for the run, e.g. `12,40,77`,
//! - `TEST_OCEN_RUNS` - e.g. `groq,sonnet-high,sonnet-medium,sonnet-low`.
//! Keys come from the credential store, as in the app.

use std::path::PathBuf;
use std::time::Instant;

use serde_json::{Value, json};

use super::*;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name}"))
}

fn open_copy() -> Connection {
    crate::secrets::init().unwrap();
    // The installed app's key (a debug build has its own `database-key-dev`).
    let key = crate::secrets::get("database-key").unwrap().expect("no database key");
    let mut conn = crate::db::open(&PathBuf::from(env("TEST_OCEN_DB")), &key).unwrap();
    crate::db::migrate(&mut conn).unwrap();
    conn
}

fn write(name: &str, v: &Value) {
    let path = PathBuf::from(env("TEST_OCEN_OUT")).join(name);
    std::fs::write(&path, serde_json::to_string_pretty(v).unwrap()).unwrap();
    println!("written {}", path.display());
}

/// Offers to choose from: rated by AI, with a real description, not in the trash.
#[test]
#[ignore]
fn ocen_wybierz() {
    let conn = open_copy();
    let mut stmt = conn
        .prepare(
            "SELECT o.id, o.title, o.company, o.city, o.stage, length(coalesce(o.description, '')),
                (SELECT group_concat(DISTINCT s.source) FROM offer_sources s WHERE s.offer_id = o.id),
                (SELECT max(score) FROM offer_scores WHERE offer_id = o.id),
                (SELECT count(*) FROM offer_filters f WHERE f.offer_id = o.id AND f.result != 'fail')
             FROM offers o WHERE o.deleted_at IS NULL AND length(coalesce(o.description, '')) >= 150
             ORDER BY o.id",
        )
        .unwrap();
    let rows: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({
                "id": r.get::<_, i64>(0)?, "title": r.get::<_, String>(1)?, "company": r.get::<_, String>(2)?,
                "city": r.get::<_, Option<String>>(3)?, "stage": r.get::<_, String>(4)?, "desc_len": r.get::<_, i64>(5)?,
                "sources": r.get::<_, Option<String>>(6)?, "best_score": r.get::<_, Option<i64>>(7)?, "fitting_profiles": r.get::<_, i64>(8)?,
            }))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let profiles: Vec<Value> = profiles::list(&conn).unwrap().iter().map(|p| json!({ "id": p.id, "name": p.name })).collect();
    let s = settings::all(&conn).unwrap();
    let keys: Vec<Value> = [Provider::Groq, Provider::Anthropic]
        .iter()
        .map(|p| json!({ "provider": p.as_str(), "key": crate::secrets::get(p.key_name().unwrap()).unwrap().is_some() }))
        .collect();
    let groq_model = configs(&s).into_iter().find(|c| c.provider == Provider::Groq).map(|c| c.model);
    println!("offers {}, profiles {}, keys {keys:?}, groq {groq_model:?}", rows.len(), profiles.len());
    write("kandydaci.json", &json!({ "offers": rows, "profiles": profiles, "keys": keys, "groq_model": groq_model }));
}

struct Run {
    name: &'static str,
    provider: Provider,
    model: &'static str,
    effort: &'static str,
}

fn run_of(name: &str) -> Run {
    match name {
        "groq" => Run { name: "groq", provider: Provider::Groq, model: "openai/gpt-oss-120b", effort: "" },
        "sonnet-high" => Run { name: "sonnet-high", provider: Provider::Anthropic, model: "claude-sonnet-5-5", effort: "high" },
        "sonnet-medium" => Run { name: "sonnet-medium", provider: Provider::Anthropic, model: "claude-sonnet-5-5", effort: "medium" },
        "sonnet-low" => Run { name: "sonnet-low", provider: Provider::Anthropic, model: "claude-sonnet-5-5", effort: "low" },
        other => panic!("unknown run {other}"),
    }
}

/// Every offer through every run; one file per run and offer, so a stop halfway keeps what is done.
#[test]
#[ignore]
fn ocen_uruchom() {
    let conn = open_copy();
    let ids: Vec<i64> = env("TEST_OCEN_IDS").split(',').map(|t| t.trim().parse().unwrap()).collect();
    let runs: Vec<Run> = env("TEST_OCEN_RUNS").split(',').map(|t| run_of(t.trim())).collect();
    tauri::async_runtime::block_on(async {
        for run in &runs {
            let key = crate::secrets::get(run.provider.key_name().unwrap()).unwrap();
            let mut ai = Ai::new(run.provider, run.model.into(), key, String::new()).expect("no key");
            if !run.effort.is_empty() {
                ai.effort = run.effort;
            }
            for id in &ids {
                let out = PathBuf::from(env("TEST_OCEN_OUT")).join(format!("{}-{id}.json", run.name));
                if out.exists() {
                    continue;
                }
                let job = load_job(&conn, *id).unwrap().expect("offer without a fitting profile");
                let offer_text = offer_block(&job.facts);
                let instructions = rate_instructions(&job.profiles, &job.experience, "pl");
                let started = Instant::now();
                let result = ai.ask_cached_usage(RATE_SYSTEM, &instructions, &offer_part(&offer_text), true).await;
                let seconds = started.elapsed().as_secs_f64();
                let v = match result {
                    Ok((answer, usage)) => {
                        let raw = client::json_object(&answer).ok();
                        let allowed: Vec<i64> = job.profiles.iter().map(|p| p.id).collect();
                        let reading = raw.as_ref().map(|r| read_answer(r, &offer_text, &allowed, "pl"));
                        let notes = reading.clone().map(|r| crate::fieldcheck::check(&draft_of(r, &offer_text, None, None), &offer_text));
                        json!({
                            "run": run.name, "offer": id, "seconds": seconds, "usage": usage, "raw": raw, "answer_if_not_json": raw.is_none().then_some(answer),
                            "scores": reading.as_ref().map(|r| &r.scores), "flags": reading.as_ref().map(|r| &r.flags),
                            "summary": reading.as_ref().map(|r| &r.summary), "field_notes": notes,
                        })
                    }
                    // Not written - the next start asks again (Groq's free tier: 8000 tokens a minute).
                    Err(e) => {
                        println!("{} {id}: {e}", run.name);
                        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                        continue;
                    }
                };
                println!("{} {id}: {:.1}s", run.name, seconds);
                std::fs::write(&out, serde_json::to_string_pretty(&v).unwrap()).unwrap();
                // About 4000 tokens a call: two a minute keep Groq under its per-minute limit.
                let pause = if run.provider == Provider::Groq { std::time::Duration::from_secs(32) } else { run.provider.pause() };
                tokio::time::sleep(pause).await;
            }
        }
    });
}

/// The offers of the run as AI gets them - for the user to rate first, without AI's scores.
#[test]
#[ignore]
fn ocen_tresc() {
    let conn = open_copy();
    let mut out = String::new();
    for (n, id) in env("TEST_OCEN_IDS").split(',').map(|t| t.trim().parse::<i64>().unwrap()).enumerate() {
        let Some(job) = load_job(&conn, id).unwrap() else {
            println!("no fitting profile: {id}");
            continue;
        };
        out.push_str(&format!("## Oferta {} (nr {id})\n\n```text\n{}\n```\n\n", n + 1, offer_block(&job.facts)));
    }
    let path = PathBuf::from(env("TEST_OCEN_OUT")).join("oferty-do-oceny.md");
    std::fs::write(&path, out).unwrap();
    println!("written {}", path.display());
}
