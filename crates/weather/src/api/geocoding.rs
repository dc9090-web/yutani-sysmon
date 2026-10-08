//! Geocoding 1.0 (SPEC §4.3): city search for the settings page.

use std::collections::HashMap;

use serde::Deserialize;

use super::Format;

/// Results asked for.
pub const LIMIT: usize = 5;

/// One search result.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub name: String,
    /// `"<state>, <country>"` or `"<country>"`.
    pub region: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Deserialize)]
struct RawPlace {
    name: String,
    #[serde(default)]
    local_names: HashMap<String, String>,
    lat: f64,
    lon: f64,
    #[serde(default)]
    country: String,
    state: Option<String>,
}

/// The results, named in `lang` where the API has that name.
pub fn places(body: &[u8], lang: &str) -> Result<Vec<Place>, Format> {
    let raw: Vec<RawPlace> = serde_json::from_slice(body)?;
    Ok(raw
        .into_iter()
        .map(|p| {
            let name = p.local_names.get(lang).filter(|n| !n.trim().is_empty()).cloned().unwrap_or(p.name);
            let region = match p.state.filter(|s| !s.is_empty()) {
                Some(state) if !p.country.is_empty() => format!("{state}, {}", p.country),
                Some(state) => state,
                None => p.country,
            };
            Place { name, region, lat: p.lat, lon: p.lon }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::fixture;

    #[test]
    fn melbourne() {
        let p = places(&fixture("geocoding/direct_melbourne.json"), "en").unwrap();
        assert_eq!(p.len(), 5);
        assert_eq!(p[0], Place { name: "Melbourne".into(), region: "Victoria, AU".into(), lat: -37.8142176, lon: 144.9631608 });
        let regions: Vec<&str> = p.iter().map(|x| x.region.as_str()).collect();
        assert_eq!(regions, ["Victoria, AU", "Florida, US", "England, GB", "Iowa, US", "Arkansas, US"]);
    }

    #[test]
    fn empty() {
        assert_eq!(places(&fixture("geocoding/direct_empty.json"), "en").unwrap(), []);
    }

    #[test]
    fn local_name_and_region_shapes() {
        let body = r#"[
            {"name":"Munich","local_names":{"de":"München","en":"Munich"},"lat":48.1,"lon":11.6,"country":"DE","state":"Bavaria"},
            {"name":"Monaco","lat":43.7,"lon":7.4,"country":"MC"},
            {"name":"Somewhere","local_names":{"de":" "},"lat":1,"lon":2,"country":"","state":"Nowhere"}
        ]"#;
        let p = places(body.as_bytes(), "de").unwrap();
        assert_eq!((p[0].name.as_str(), p[0].region.as_str()), ("München", "Bavaria, DE"));
        assert_eq!((p[1].name.as_str(), p[1].region.as_str()), ("Monaco", "MC"));
        assert_eq!((p[2].name.as_str(), p[2].region.as_str()), ("Somewhere", "Nowhere"));
    }
}
