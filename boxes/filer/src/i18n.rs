//! Teksty interfejsu z `boxes/filer/i18n/` (wspólny format Shrimp, zob. boxes/shrimp-i18n).
//! W testach zawsze po polsku, żeby asercje nie zależały od języka maszyny.
const NS: &str = "filer";

shrimp_i18n::texts!();

fn ns() -> &'static str {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| shrimp_i18n::register(NS, SHRIMP_I18N_KEYS, SHRIMP_I18N_LANGS));
    NS
}

#[cfg(not(test))]
const LANG: Option<&str> = None;
#[cfg(test)]
const LANG: Option<&str> = Some("pl");

pub fn tr(key: &'static str) -> &'static str {
    match LANG {
        Some(l) => shrimp_i18n::tr_in(ns(), l, key),
        None => shrimp_i18n::tr(ns(), key),
    }
}

/// `tr` z podstawieniem `{name}` itd.
pub fn trf(key: &'static str, args: &[(&str, &str)]) -> String {
    shrimp_i18n::fill(tr(key), args)
}

/// Forma liczby mnogiej `<klucz>.<forma>` z `{n}` = liczba.
pub fn trn(key: &str, n: u64, args: &[(&str, &str)]) -> String {
    match LANG {
        Some(l) => shrimp_i18n::trn_in(ns(), l, key, n, args),
        None => shrimp_i18n::trn(ns(), key, n, args),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn translations_consistent() {
        let bad = shrimp_i18n::check(super::ns());
        assert!(bad.is_empty(), "{bad:#?}");
    }

    /// Każdy klucz użyty w kodzie (tr/trf/trn) istnieje w _keys.tsv.
    #[test]
    fn every_used_key_exists() {
        let keys = include_str!("../i18n/_keys.tsv");
        let known: std::collections::HashSet<&str> = keys.lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.split('\t').nth(1)).collect();
        let mut missing = vec![];
        for (name, src) in crate::SOURCES {
            for pat in ["tr(\"", "trf(\"", "trn(\""] {
                for part in src.split(pat).skip(1) {
                    let k = part.split('"').next().unwrap_or("");
                    let ok = if pat == "trn(\"" { known.iter().any(|x| x.starts_with(&format!("{k}."))) } else { known.contains(k) };
                    if !ok {
                        missing.push(format!("{name}: {k}"));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "brak w _keys.tsv: {missing:#?}");
    }
}
