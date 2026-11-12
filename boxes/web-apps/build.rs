// Wbudowuje ikony z data/icons/*.png (pobiera je tools/verify.py w CI) w bibliotekę,
// żeby lista i tworzenie aplikacji działały bez sieci.
use std::{env, fs, path::Path};

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/icons");
    println!("cargo:rerun-if-changed={}", dir.display());
    println!("cargo:rerun-if-changed=data/webapps.json");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".png"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    let mut out = String::from("pub(crate) static ICONS: &[(&str, &[u8])] = &[\n");
    for n in names {
        let id = n.trim_end_matches(".png");
        out += &format!("    ({id:?}, include_bytes!({:?})),\n", dir.join(&n).display().to_string());
    }
    out += "];\n";
    let out_dir = env::var("OUT_DIR").unwrap();
    fs::write(Path::new(&out_dir).join("icons.rs"), out).unwrap();
    fs::write(Path::new(&out_dir).join("langs.rs"), langs()).unwrap();
}

/// Tłumaczenia z i18n/*.txt (box web-apps) oraz tablice znaczników z boxes/i18n-core/langs.ts (jedno źródło prawdy dla
/// wszystkich aplikacji): LANGS (tag → tekst), TEXT_FALLBACK (tag → język zapasowy tekstów),
/// ALIASES (dodatkowy znacznik → tag) i LOCALES (locale glibc → tag).
fn langs() -> String {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let texts = manifest.join("i18n");
    let core = manifest.join("../i18n-core");
    println!("cargo:rerun-if-changed={}", core.join("langs.ts").display());
    println!("cargo:rerun-if-changed={}", texts.display());
    let mut files: Vec<String> = fs::read_dir(&texts)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    files.retain(|n| n.ends_with(".txt"));
    files.sort();
    let mut out = String::from("pub(crate) static LANGS: &[(&str, &str)] = &[\n");
    for n in &files {
        let path = fs::canonicalize(texts.join(n)).unwrap();
        out += &format!("    ({:?}, include_str!({:?})),\n", n.trim_end_matches(".txt"), path.display().to_string());
    }
    out += "];\n";
    let ts = fs::read_to_string(core.join("langs.ts")).unwrap_or_default();
    let (mut fallback, mut aliases) = (String::new(), String::new());
    for row in ts_rows(&ts, "const ROWS: Row[] = [") {
        let q = quoted(&row);
        let Some(tag) = q.first() else { continue };
        if let Some(i) = row.find("text: \"") {
            fallback += &format!("    ({tag:?}, {:?}),\n", quoted(&row[i + 6..])[0]);
        }
        if let Some(i) = row.find("match: [") {
            let list = &row[i + 8..row[i..].find(']').map(|j| i + j).unwrap_or(row.len())];
            for m in quoted(list) {
                aliases += &format!("    ({m:?}, {tag:?}),\n");
            }
        }
    }
    let mut locales = String::new();
    for row in ts_rows(&ts, "export const GNOME_LOCALES: [locale: string, tag: string, name?: string][] = [") {
        let q = quoted(&row);
        if q.len() >= 2 {
            locales += &format!("    ({:?}, {:?}),\n", q[0], q[1]);
        }
    }
    out += &format!("pub(crate) static TEXT_FALLBACK: &[(&str, &str)] = &[\n{fallback}];\n");
    out += &format!("pub(crate) static ALIASES: &[(&str, &str)] = &[\n{aliases}];\n");
    out += &format!("pub(crate) static LOCALES: &[(&str, &str)] = &[\n{locales}];\n");
    out
}

/// Wiersze `[...]` tablicy zaczynającej się po `start` (zagnieżdżone nawiasy zostają w wierszu).
fn ts_rows(ts: &str, start: &str) -> Vec<String> {
    let Some(i) = ts.find(start) else { return vec![] };
    let (mut rows, mut cur, mut depth, mut in_str) = (vec![], String::new(), 0, false);
    for line in ts[i + start.len()..].lines() {
        if depth == 0 && line.trim_start().starts_with("//") {
            continue;
        }
        if depth == 0 && line.starts_with("];") {
            break;
        }
        for c in line.chars().chain(std::iter::once(' ')) {
            if c == '"' {
                in_str = !in_str;
            }
            if !in_str && c == '[' {
                depth += 1;
                if depth == 1 {
                    continue;
                }
            }
            if !in_str && c == ']' {
                depth -= 1;
                if depth == 0 {
                    rows.push(std::mem::take(&mut cur));
                    continue;
                }
            }
            if depth > 0 {
                cur.push(c);
            }
        }
    }
    rows
}

fn quoted(s: &str) -> Vec<String> {
    s.split('"').skip(1).step_by(2).map(String::from).collect()
}
