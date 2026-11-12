//! Teksty interfejsu z `boxes/web-apps/i18n/` (ten sam format co Setup i Shrimp Install:
//! `_keys.tsv` = id⇥klucz⇥angielski, `<tag>.txt` = id⇥tłumaczenie, brak linii = angielski).
use std::collections::HashMap;
use std::sync::OnceLock;

const KEYS: &str = include_str!("../i18n/_keys.tsv");
include!(concat!(env!("OUT_DIR"), "/langs.rs"));

/// Język z LANGUAGE / LC_ALL / LC_MESSAGES / LANG, np. „pl”.
pub fn lang() -> String {
    static LANG: OnceLock<String> = OnceLock::new();
    LANG.get_or_init(|| {
        for var in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(v) = std::env::var(var) {
                let first = v.split(':').next().unwrap_or("");
                let tag = first.split(['.', '@']).next().unwrap_or("");
                if !tag.is_empty() && tag != "C" && tag != "POSIX" {
                    return tag.split('_').next().unwrap_or("en").to_lowercase();
                }
            }
        }
        "en".into()
    })
    .clone()
}

/// Nasz znacznik tłumaczenia (jak w boxes/i18n-core/langs.ts) dla locale z środowiska, np. „pt_BR.UTF-8” → „pt-br”,
/// „sr_RS@latin” → „sr-latn”, „zh_TW” → „zh-hant”. Bez dopasowania: „en”.
pub fn tag_for(locale: &str) -> String {
    let base = locale.split('.').next().unwrap_or("");
    let (main, modifier) = match base.split_once('@') {
        Some((m, r)) => (m, Some(r)),
        None => (base, None),
    };
    let main = main.replace('-', "_");
    let has = |t: &str| LANGS.iter().any(|(l, _)| *l == t) || t == "en";
    let mut tries: Vec<String> = vec![];
    if let Some(m) = modifier {
        tries.push(format!("{main}@{m}"));
    }
    tries.push(main.clone());
    for t in &tries {
        if let Some((_, tag)) = LOCALES.iter().find(|(l, _)| l.eq_ignore_ascii_case(t)) {
            return tag.to_string();
        }
    }
    let lower = main.to_lowercase().replace('_', "-");
    let lang_only = lower.split('-').next().unwrap_or("").to_string();
    let mut cands = vec![lower.clone()];
    if let Some(m) = modifier {
        cands.insert(0, format!("{lower}-{}", m.to_lowercase()));
    }
    cands.push(lang_only);
    for c in cands {
        if has(&c) {
            return c;
        }
        if let Some((_, tag)) = ALIASES.iter().find(|(a, _)| *a == c) {
            return tag.to_string();
        }
    }
    "en".into()
}

fn locale_env() -> String {
    for var in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            let first = v.split(':').next().unwrap_or("");
            let tag = first.split('.').next().unwrap_or("");
            if !tag.is_empty() && tag != "C" && tag != "POSIX" {
                return first.to_string();
            }
        }
    }
    String::new()
}

fn table() -> &'static HashMap<String, String> {
    static T: OnceLock<HashMap<String, String>> = OnceLock::new();
    T.get_or_init(|| build(&tag_for(&locale_env())))
}

fn text_of(tag: &str) -> Option<&'static str> {
    LANGS.iter().find(|(t, _)| *t == tag).map(|(_, txt)| *txt)
}

fn build(lang: &str) -> HashMap<String, String> {
    let mut by_id: HashMap<&str, (&str, &str)> = HashMap::new();
    for line in KEYS.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        let mut p = line.splitn(3, '\t');
        if let (Some(id), Some(key), Some(en)) = (p.next(), p.next(), p.next()) {
            by_id.insert(id, (key, en));
        }
    }
    let mut out: HashMap<String, String> =
        by_id.values().map(|(k, en)| (k.to_string(), en.to_string())).collect();
    // Najpierw język zapasowy tekstów (np. keczua → hiszpański), potem sam język.
    let fallback = TEXT_FALLBACK.iter().find(|(t, _)| *t == lang).map(|(_, f)| *f).filter(|f| *f != "en");
    for txt in [fallback.and_then(text_of), text_of(lang)].into_iter().flatten() {
        for line in txt.lines() {
            if let Some((id, tr)) = line.split_once('\t') {
                if let Some((key, _)) = by_id.get(id) {
                    out.insert(key.to_string(), tr.to_string());
                }
            }
        }
    }
    out
}

pub fn tr(key: &str) -> String {
    table().get(key).cloned().unwrap_or_else(|| key.to_string())
}

/// `tr` z podstawieniem `{name}` itd.
pub fn trf(key: &str, args: &[(&str, &str)]) -> String {
    let mut s = tr(key);
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polish_and_english() {
        assert_eq!(build("pl")["button.create"], "Utwórz");
        assert_eq!(build("en")["button.create"], "Create");
        assert_eq!(build("xx")["toast.created"], "{name} was added to your apps");
        assert_eq!(tag_for("pt_BR.UTF-8"), "pt-br");
        assert_eq!(tag_for("pl_PL.UTF-8"), "pl");
        assert_eq!(tag_for("sr_RS@latin"), "sr-latn");
        assert_eq!(tag_for("zh_TW.UTF-8"), "zh-hant");
        assert_eq!(tag_for("C"), "en");
        assert_eq!(tag_for("xx_XX"), "en");
        // Każdy id z pl.txt ma klucz.
        let ids: Vec<&str> = KEYS.lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.split('\t').next()).collect();
        for (_, txt) in LANGS {
            for l in txt.lines() {
                assert!(ids.contains(&l.split('\t').next().unwrap()), "{l}");
            }
        }
    }

    #[test]
    fn every_language_complete_with_same_placeholders() {
        let en = build("en");
        let ph = |s: &str| {
            let mut v: Vec<String> = s.split('{').skip(1).filter_map(|r| r.split('}').next()).map(String::from).collect();
            v.sort();
            v
        };
        assert!(LANGS.len() > 100, "oczekiwano tłumaczeń na języki GNOME");
        for (tag, _) in LANGS {
            let t = build(tag);
            for (k, v) in &en {
                assert_eq!(ph(&t[k]), ph(v), "{tag}: {k}");
                if *tag != "en" {
                    assert!(!t[k].is_empty(), "{tag}: {k} puste");
                }
            }
            assert_eq!(text_of(tag).unwrap().lines().filter(|l| !l.is_empty()).count(), en.len(), "{tag}: niepełne");
        }
    }
}
