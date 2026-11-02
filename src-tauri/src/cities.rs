//! Approximate centres of Polish cities - enough to check "up to 30 km from my city"
//! without any online map service. A city missing here never rejects an offer;
//! the filter answers "unsure" instead.

use crate::dedup::normalize;

/// (normalized name, latitude, longitude). Several names may point to one place (aliases).
const CITIES: &[(&str, f64, f64)] = &[
    ("warszawa", 52.23, 21.01),
    ("warsaw", 52.23, 21.01),
    ("krakow", 50.06, 19.94),
    ("cracow", 50.06, 19.94),
    ("lodz", 51.76, 19.46),
    ("wroclaw", 51.11, 17.03),
    ("poznan", 52.41, 16.93),
    ("gdansk", 54.35, 18.65),
    ("szczecin", 53.43, 14.55),
    ("bydgoszcz", 53.12, 18.01),
    ("lublin", 51.25, 22.57),
    ("bialystok", 53.13, 23.16),
    ("katowice", 50.26, 19.02),
    ("gdynia", 54.52, 18.53),
    ("sopot", 54.44, 18.56),
    ("czestochowa", 50.81, 19.12),
    ("radom", 51.40, 21.15),
    ("torun", 53.01, 18.60),
    ("sosnowiec", 50.29, 19.10),
    ("rzeszow", 50.04, 22.00),
    ("kielce", 50.87, 20.63),
    ("gliwice", 50.29, 18.67),
    ("olsztyn", 53.78, 20.49),
    ("zabrze", 50.32, 18.79),
    ("bielsko biala", 49.82, 19.04),
    ("bytom", 50.35, 18.92),
    ("zielona gora", 51.94, 15.51),
    ("rybnik", 50.10, 18.55),
    ("ruda slaska", 50.26, 18.86),
    ("opole", 50.67, 17.93),
    ("tychy", 50.12, 18.99),
    ("gorzow wielkopolski", 52.73, 15.24),
    ("gorzow wlkp", 52.73, 15.24),
    ("elblag", 54.16, 19.40),
    ("plock", 52.55, 19.70),
    ("dabrowa gornicza", 50.32, 19.19),
    ("walbrzych", 50.77, 16.28),
    ("wloclawek", 52.65, 19.07),
    ("tarnow", 50.01, 20.99),
    ("chorzow", 50.30, 18.95),
    ("koszalin", 54.19, 16.17),
    ("kalisz", 51.76, 18.09),
    ("legnica", 51.21, 16.16),
    ("grudziadz", 53.48, 18.75),
    ("slupsk", 54.46, 17.03),
    ("jaworzno", 50.20, 19.27),
    ("jastrzebie zdroj", 49.96, 18.60),
    ("nowy sacz", 49.62, 20.69),
    ("jelenia gora", 50.90, 15.73),
    ("siedlce", 52.17, 22.29),
    ("myslowice", 50.24, 19.14),
    ("konin", 52.22, 18.25),
    ("pila", 53.15, 16.74),
    ("piotrkow trybunalski", 51.40, 19.70),
    ("inowroclaw", 52.80, 18.26),
    ("lubin", 51.40, 16.20),
    ("ostrow wielkopolski", 51.65, 17.81),
    ("ostrow wlkp", 51.65, 17.81),
    ("suwalki", 54.10, 22.93),
    ("gniezno", 52.53, 17.60),
    ("stargard", 53.34, 15.05),
    ("pruszkow", 52.17, 20.81),
    ("piaseczno", 52.08, 21.02),
    ("legionowo", 52.40, 20.93),
    ("otwock", 52.10, 21.26),
    ("pabianice", 51.66, 19.35),
    ("zgierz", 51.86, 19.41),
    ("glogow", 51.66, 16.08),
    ("zamosc", 50.72, 23.25),
    ("leszno", 51.84, 16.57),
    ("przemysl", 49.78, 22.77),
    ("lomza", 53.18, 22.06),
    ("swidnica", 50.84, 16.49),
    ("mielec", 50.29, 21.42),
    ("tczew", 54.09, 18.78),
    ("krosno", 49.69, 21.77),
    ("skierniewice", 51.95, 20.16),
    ("ostroleka", 53.09, 21.57),
    ("ciechanow", 52.88, 20.62),
    ("pulawy", 51.42, 21.97),
    ("kutno", 52.23, 19.36),
    ("chelm", 51.14, 23.47),
    ("biala podlaska", 52.03, 23.12),
    ("tarnobrzeg", 50.57, 21.68),
    ("stalowa wola", 50.58, 22.05),
    ("oswiecim", 50.04, 19.22),
    ("wieliczka", 49.99, 20.06),
    ("skawina", 49.98, 19.83),
    ("zakopane", 49.30, 19.95),
];

pub fn coordinates(name: &str) -> Option<(f64, f64)> {
    let key = normalize(name);
    CITIES.iter().find(|(n, _, _)| *n == key).map(|&(_, lat, lon)| (lat, lon))
}

/// Known cities mentioned in an offer's city field, e.g. "Warszawa, Kraków" or
/// "Gdańsk (Oliwa)". Returns normalized names.
pub fn mentioned(city_field: &str) -> Vec<&'static str> {
    let text = format!(" {} ", normalize(city_field));
    CITIES
        .iter()
        .map(|(n, _, _)| *n)
        .filter(|n| text.contains(&format!(" {n} ")))
        .collect()
}

/// Great-circle distance in kilometres.
pub fn distance_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lon1) = (a.0.to_radians(), a.1.to_radians());
    let (lat2, lon2) = (b.0.to_radians(), b.1.to_radians());
    let h = ((lat2 - lat1) / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * ((lon2 - lon1) / 2.0).sin().powi(2);
    2.0 * 6371.0 * h.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_cities_with_polish_letters_and_aliases() {
        assert!(coordinates("Łódź").is_some());
        assert_eq!(coordinates("Warsaw"), coordinates("Warszawa"));
        assert!(coordinates("Atlantyda").is_none());
        assert_eq!(mentioned("Warszawa, Kraków"), vec!["warszawa", "krakow"]);
        assert_eq!(mentioned("Gdańsk (Oliwa)"), vec!["gdansk"]);
        // "Lubin" is not "Lublin".
        assert_eq!(mentioned("Lublin"), vec!["lublin"]);
    }

    #[test]
    fn distances_are_roughly_right() {
        let waw = coordinates("Warszawa").unwrap();
        let pruszkow = coordinates("Pruszków").unwrap();
        let krakow = coordinates("Kraków").unwrap();
        assert!(distance_km(waw, pruszkow) < 20.0);
        let far = distance_km(waw, krakow);
        assert!((240.0..270.0).contains(&far), "{far}");
    }
}
