//! Teksty interfejsu z `boxes/cocktail/i18n/` (wspólny format Shrimp, zob. boxes/shrimp-i18n).
//! W testach zawsze po polsku, żeby asercje nie zależały od języka maszyny.
const NS: &str = "cocktail";

shrimp_i18n::texts!();

/// Przestrzeń tekstów aplikacji, zarejestrowana przy pierwszym użyciu.
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

/// Liczba mnoga: klucze `<key>.one/.few/.many/.other`, `{n}` = liczba.
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
}
