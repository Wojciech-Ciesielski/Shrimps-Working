//! Teksty interfejsu aplikacji GTK Shrimp (Shrimp Apps, Panko, Cocktail) z katalogu `i18n/` boxa aplikacji.
//! Ten sam format co Setup, Shrimp Install i Web Apps: `_keys.tsv` = id⇥klucz⇥angielski,
//! `<znacznik>.txt` = id⇥tłumaczenie, brak linii = angielski (albo język zapasowy z `text` w langs.ts).
//!
//! Liczba mnoga: klucze `<klucz>.one`, `.few`, `.many`, `.other` i [`trn`]; formę wybiera [`plural`]
//! według reguł CLDR dla języka (brak formy = `.other`, potem angielski).
//!
//! Aplikacja (box) trzyma teksty u siebie: `build.rs` woła [`codegen::write`]`("i18n")`, a moduł
//! tekstów aplikacji [`texts!`] i [`register`] (biblioteka nie zna aplikacji, zależność idzie w jedną stronę).
use std::collections::HashMap;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/tables.rs"));

type Table = HashMap<&'static str, &'static str>;

/// Locale z LANGUAGE / LC_ALL / LC_MESSAGES / LANG (pierwszy niepusty, bez C/POSIX).
pub fn locale_env() -> String {
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

/// Nasz znacznik (jak w boxes/i18n-core/langs.ts) dla locale, np. „pt_BR.UTF-8” → „pt-br”, „sr_RS@latin” → „sr-latn”.
pub fn tag_for(locale: &str) -> String {
    let base = locale.split('.').next().unwrap_or("");
    let (main, modifier) = match base.split_once('@') {
        Some((m, r)) => (m, Some(r)),
        None => (base, None),
    };
    let main = main.replace('-', "_");
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
        if c == "en" || TAGS.contains(&c.as_str()) {
            return c;
        }
        if let Some((_, tag)) = ALIASES.iter().find(|(a, _)| *a == c) {
            return tag.to_string();
        }
    }
    "en".into()
}

/// Znacznik języka bieżącej sesji (SHRIMP_LANG wymusza, np. do zrzutów ekranu).
pub fn lang() -> &'static str {
    static L: OnceLock<String> = OnceLock::new();
    L.get_or_init(|| match std::env::var("SHRIMP_LANG") {
        Ok(t) if !t.is_empty() => t,
        _ => tag_for(&locale_env()),
    })
}

type Langs = &'static [(&'static str, &'static str)];

fn registry() -> &'static std::sync::Mutex<HashMap<&'static str, (&'static str, Langs)>> {
    static R: OnceLock<std::sync::Mutex<HashMap<&'static str, (&'static str, Langs)>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

/// Rejestruje teksty przestrzeni `ns`: `_keys.tsv` i pary (znacznik, `<znacznik>.txt`), zwykle z [`texts!`].
pub fn register(ns: &'static str, keys: &'static str, langs: Langs) {
    registry().lock().unwrap().insert(ns, (keys, langs));
}

fn namespace(ns: &str) -> (&'static str, Langs) {
    registry()
        .lock()
        .unwrap()
        .get(ns)
        .copied()
        .unwrap_or_else(|| panic!("przestrzeń tekstów {ns} nie jest zarejestrowana (shrimp_i18n::register)"))
}

/// Wstawia `SHRIMP_I18N_KEYS` i `SHRIMP_I18N_LANGS` wygenerowane przez [`codegen::write`] w build.rs aplikacji.
#[macro_export]
macro_rules! texts {
    () => {
        include!(concat!(env!("OUT_DIR"), "/shrimp_i18n_texts.rs"));
    };
}

/// Dla build.rs aplikacji: wbudowuje `_keys.tsv` i `<znacznik>.txt` z katalogu tekstów boxa.
pub mod codegen {
    use std::{env, fs, path::Path};

    /// `dir` względem katalogu aplikacji (CARGO_MANIFEST_DIR), zwykle `"i18n"`.
    pub fn write(dir: &str) {
        let manifest = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        let dir = Path::new(&manifest).join(dir);
        println!("cargo:rerun-if-changed={}", dir.display());
        let keys = fs::canonicalize(dir.join("_keys.tsv")).unwrap_or_else(|e| panic!("{}/_keys.tsv: {e}", dir.display()));
        let mut files: Vec<String> = fs::read_dir(&dir)
            .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        files.retain(|n| n.ends_with(".txt"));
        files.sort();
        let mut out = format!(
            "#[allow(dead_code)]\npub(crate) static SHRIMP_I18N_KEYS: &str = include_str!({:?});\n#[allow(dead_code)]\npub(crate) static SHRIMP_I18N_LANGS: &[(&str, &str)] = &[\n",
            keys.display().to_string()
        );
        for n in &files {
            let p = fs::canonicalize(dir.join(n)).unwrap();
            println!("cargo:rerun-if-changed={}", p.display());
            out += &format!("    ({:?}, include_str!({:?})),\n", n.trim_end_matches(".txt"), p.display().to_string());
        }
        out += "];\n";
        fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("shrimp_i18n_texts.rs"), out).unwrap();
    }
}

/// Tabela klucz → tekst dla przestrzeni i języka (angielski, potem język zapasowy, potem sam język).
pub fn build(ns: &str, lang: &str) -> Table {
    let (keys, langs) = namespace(ns);
    let mut by_id: HashMap<&str, (&'static str, &'static str)> = HashMap::new();
    for line in keys.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        let mut p = line.splitn(3, '\t');
        if let (Some(id), Some(key), Some(en)) = (p.next(), p.next(), p.next()) {
            by_id.insert(id, (key, en));
        }
    }
    let mut out: Table = by_id.values().map(|(k, en)| (*k, *en)).collect();
    let text_of = |tag: &str| langs.iter().find(|(t, _)| *t == tag).map(|(_, txt)| *txt);
    let fallback = TEXT_FALLBACK.iter().find(|(t, _)| *t == lang).map(|(_, f)| *f).filter(|f| *f != "en");
    for txt in [fallback.and_then(text_of), text_of(lang)].into_iter().flatten() {
        for line in txt.lines() {
            if let Some((id, tr)) = line.split_once('\t') {
                if let Some((key, _)) = by_id.get(id) {
                    out.insert(key, tr);
                }
            }
        }
    }
    out
}

fn table(ns: &str, lang: &str) -> &'static Table {
    use std::sync::Mutex;
    static T: OnceLock<Mutex<HashMap<(String, String), &'static Table>>> = OnceLock::new();
    let mut m = T.get_or_init(Default::default).lock().unwrap();
    m.entry((ns.to_string(), lang.to_string())).or_insert_with(|| Box::leak(Box::new(build(ns, lang))))
}

/// Tekst dla klucza; bez klucza w `_keys.tsv` zwraca sam klucz (widać go wtedy w oknie i w testach).
pub fn tr(ns: &str, key: &'static str) -> &'static str {
    tr_in(ns, lang(), key)
}

/// `tr` w podanym języku (testy aplikacji sprawdzają teksty po polsku).
pub fn tr_in(ns: &str, lang: &str, key: &'static str) -> &'static str {
    table(ns, lang).get(key).copied().unwrap_or(key)
}

/// `tr` z podstawieniem `{name}` itd.
pub fn trf(ns: &str, key: &'static str, args: &[(&str, &str)]) -> String {
    fill(tr(ns, key), args)
}

pub fn trf_in(ns: &str, lang: &str, key: &'static str, args: &[(&str, &str)]) -> String {
    fill(tr_in(ns, lang, key), args)
}

pub fn fill(s: &str, args: &[(&str, &str)]) -> String {
    let mut s = s.to_string();
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

/// Forma liczby mnogiej `<klucz>.<forma>` z `{n}` podstawionym liczbą (i resztą `args`).
pub fn trn(ns: &str, key: &str, n: u64, args: &[(&str, &str)]) -> String {
    trn_in(ns, lang(), key, n, args)
}

pub fn trn_in(ns: &str, lang: &str, key: &str, n: u64, args: &[(&str, &str)]) -> String {
    let t = table(ns, lang);
    let pick = |cat: &str| t.get(format!("{key}.{cat}").as_str()).copied();
    let s = pick(plural(lang, n)).or_else(|| pick("other")).or_else(|| pick("one")).unwrap_or("{n}");
    let n = n.to_string();
    let mut all = vec![("n", n.as_str())];
    all.extend_from_slice(args);
    fill(s, &all)
}

/// Forma liczby mnogiej (CLDR, liczby całkowite): one / few / many / other.
pub fn plural(lang: &str, n: u64) -> &'static str {
    let base = lang.split('-').next().unwrap_or(lang);
    let (m10, m100) = (n % 10, n % 100);
    match base {
        // bez liczby mnogiej
        "ja" | "zh" | "ko" | "th" | "vi" | "id" | "ms" | "my" | "km" | "lo" | "yo" | "ig" | "dz" | "jv" | "su" => "other",
        "pl" => {
            if n == 1 {
                "one"
            } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
                "few"
            } else {
                "many"
            }
        }
        "ru" | "uk" | "be" => {
            if m10 == 1 && m100 != 11 {
                "one"
            } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
                "few"
            } else {
                "many"
            }
        }
        "sr" | "hr" | "bs" => {
            if m10 == 1 && m100 != 11 {
                "one"
            } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
                "few"
            } else {
                "other"
            }
        }
        "cs" | "sk" => match n {
            1 => "one",
            2..=4 => "few",
            _ => "other",
        },
        "fr" | "pt" | "hi" | "bn" | "fa" | "am" | "zu" | "kab" | "hy" | "gu" | "kn" | "as" => {
            if n <= 1 {
                "one"
            } else {
                "other"
            }
        }
        _ => {
            if n == 1 {
                "one"
            } else {
                "other"
            }
        }
    }
}

/// Sprawdzenia wspólne dla testów aplikacji: każdy id w tłumaczeniach istnieje w `_keys.tsv`,
/// placeholdery są te same co w angielskim, a teksty nie są puste. Zwraca listę problemów.
pub fn check(ns: &str) -> Vec<String> {
    let (keys, langs) = namespace(ns);
    let ids: Vec<&str> = keys.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()).filter_map(|l| l.split('\t').next()).collect();
    let mut seen = std::collections::HashSet::new();
    let mut bad = vec![];
    for id in &ids {
        if !seen.insert(*id) {
            bad.push(format!("{ns}: id {id} dwa razy w _keys.tsv"));
        }
    }
    let ph = |s: &str| {
        let mut v: Vec<String> = s.split('{').skip(1).filter_map(|r| r.split('}').next()).map(String::from).collect();
        v.sort();
        v
    };
    let en = build(ns, "en");
    for (tag, txt) in langs {
        for l in txt.lines().filter(|l| !l.is_empty()) {
            let id = l.split('\t').next().unwrap_or("");
            if !ids.contains(&id) {
                bad.push(format!("{ns}/{tag}: nieznany id {id}"));
            }
        }
        let t = build(ns, tag);
        for (k, v) in &en {
            // formy liczby mnogiej mogą mieć inne placeholdery tylko w zakresie {n}
            if ph(t[k]) != ph(v) && !(k.contains('.') && ph(t[k]).iter().all(|p| ph(v).contains(p) || p == "n")) {
                bad.push(format!("{ns}/{tag}: {k}: placeholdery {:?} zamiast {:?}", ph(t[k]), ph(v)));
            }
            if t[k].is_empty() {
                bad.push(format!("{ns}/{tag}: {k} puste"));
            }
        }
    }
    bad
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags() {
        assert_eq!(tag_for("pt_BR.UTF-8"), "pt-br");
        assert_eq!(tag_for("pl_PL.UTF-8"), "pl");
        assert_eq!(tag_for("sr_RS@latin"), "sr-latn");
        assert_eq!(tag_for("C"), "en");
        assert_eq!(tag_for("xx_XX"), "en");
    }

    #[test]
    fn plurals() {
        let f = |n| plural("pl", n);
        assert_eq!((f(1), f(2), f(5), f(12), f(22), f(25)), ("one", "few", "many", "many", "few", "many"));
        assert_eq!((plural("en", 1), plural("en", 0)), ("one", "other"));
        assert_eq!((plural("ru", 21), plural("ru", 11)), ("one", "many"));
        assert_eq!(plural("ja", 1), "other");
    }

    // Teksty aplikacji sprawdzają ich własne testy (check w i18n.rs każdej aplikacji).
    #[test]
    fn registered_namespace() {
        register("test", "1\thello\tHello {name}\n2\tn.one\t{n} file\n3\tn.other\t{n} files\n", &[("pl", "1\tCześć {name}\n")]);
        assert_eq!(tr_in("test", "pl", "hello"), "Cześć {name}");
        assert_eq!(tr_in("test", "de", "hello"), "Hello {name}");
        assert_eq!(trn_in("test", "en", "n", 2, &[]), "2 files");
        assert!(check("test").is_empty(), "{:?}", check("test"));
    }
}
