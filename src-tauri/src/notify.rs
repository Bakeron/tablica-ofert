//! Windows notifications (never e-mail). Texts follow the app language; "system" = Windows language.

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::commands::{AppState, with_db};
use crate::settings;

/// Polish unless the user chose English or Windows runs in another language.
pub fn polish(state: &AppState) -> bool {
    match with_db(state, |c| settings::get(c, "language")).as_deref() {
        Ok("pl") => true,
        Ok("en") => false,
        _ => sys_locale::get_locale().is_none_or(|l| l.to_lowercase().starts_with("pl")),
    }
}

pub fn show(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("notification failed: {e}");
    }
}

/// "3 nowe oferty w Zebranych" (the tab they land in) - Polish plural forms.
pub fn new_offers_text(count: i64, pl: bool) -> (String, String) {
    if !pl {
        let noun = if count == 1 { "offer" } else { "offers" };
        return (format!("{count} new {noun} in Collected"), "Tablica Ofert collected new offers that fit your profiles.".into());
    }
    let last = count % 10;
    let teen = (12..=14).contains(&(count % 100));
    let noun = if count == 1 {
        "nowa oferta"
    } else if (2..=4).contains(&last) && !teen {
        "nowe oferty"
    } else {
        "nowych ofert"
    };
    (format!("{count} {noun} w Zebranych"), "Tablica Ofert zebrała nowe oferty pasujące do Twoich profili.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polish_plurals() {
        assert_eq!(new_offers_text(1, true).0, "1 nowa oferta w Zebranych");
        assert_eq!(new_offers_text(3, true).0, "3 nowe oferty w Zebranych");
        assert_eq!(new_offers_text(5, true).0, "5 nowych ofert w Zebranych");
        assert_eq!(new_offers_text(13, true).0, "13 nowych ofert w Zebranych");
        assert_eq!(new_offers_text(22, true).0, "22 nowe oferty w Zebranych");
        assert_eq!(new_offers_text(1, false).0, "1 new offer in Collected");
    }
}
