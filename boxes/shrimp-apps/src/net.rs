//! Sieć: obrazy (ikony, zrzuty ekranu) z pamięcią podręczną i ranking popularności Flathub.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const USER_AGENT: &str = concat!("shrimp-apps/", env!("CARGO_PKG_VERSION"));
const POPULAR_URL: &str = "https://flathub.org/api/v2/collection/popular?page=1&per_page=1000";
const MAX_BYTES: u64 = 25 * 1024 * 1024;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .user_agent(USER_AGENT)
        .build()
        .into()
}

pub fn get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let mut resp = agent().get(url).call().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    resp.body_mut()
        .as_reader()
        .take(MAX_BYTES)
        .read_to_end(&mut out)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

pub fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("shrimp-apps")
}

/// Ścieżka obrazu w pamięci podręcznej; pobiera go, jeśli go nie ma. Wołać poza wątkiem GTK.
pub fn cached_image(url: &str) -> Result<PathBuf, String> {
    let mut h = DefaultHasher::new();
    url.hash(&mut h);
    let ext = url.rsplit('.').next().filter(|e| e.len() <= 4).unwrap_or("img");
    let dir = cache_dir().join("images");
    let path = dir.join(format!("{:016x}.{ext}", h.finish()));
    if path.exists() {
        return Ok(path);
    }
    let data = get_bytes(url)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, data).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Najpopularniejsze aplikacje Flathub (identyfikatory od najczęściej instalowanej).
/// Wynik zapisany na dzień; bez sieci zostaje ostatni zapisany albo pusta lista.
pub fn popular() -> Vec<String> {
    let file = cache_dir().join("popular.json");
    let fresh = std::fs::metadata(&file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|m| SystemTime::now().duration_since(m).ok())
        .is_some_and(|age| age < Duration::from_secs(24 * 3600));
    if !fresh {
        if let Ok(body) = get_bytes(POPULAR_URL) {
            if !parse_popular(&body).is_empty() {
                let _ = std::fs::create_dir_all(cache_dir());
                let _ = std::fs::write(&file, &body);
            }
        }
    }
    std::fs::read(&file).map(|b| parse_popular(&b)).unwrap_or_default()
}

fn parse_popular(body: &[u8]) -> Vec<String> {
    let v: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let list = v.get("hits").and_then(|h| h.as_array()).or_else(|| v.as_array());
    list.map(|a| {
        a.iter()
            .filter_map(|x| x.get("app_id").and_then(|i| i.as_str()).or_else(|| x.as_str()))
            .map(String::from)
            .collect()
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn popular_formats() {
        assert_eq!(
            super::parse_popular(br#"{"hits":[{"app_id":"a.b.C","name":"C"},{"app_id":"d.e.F"}]}"#),
            vec!["a.b.C", "d.e.F"]
        );
        assert_eq!(super::parse_popular(br#"["x.y.Z"]"#), vec!["x.y.Z"]);
        assert!(super::parse_popular(b"<html>").is_empty());
    }
}
