//! Adres strony pomocy z `boxes/help-links/help.json` (wspólny plik dla wszystkich programów Shrimp).
//! Dopóki `base` jest puste, przycisk pomocy jest nieaktywny.

const HELP_JSON: &str = include_str!("../../help-links/help.json");

#[derive(serde::Deserialize)]
struct Help {
    base: String,
    topics: std::collections::HashMap<String, String>,
}

pub fn url() -> Option<String> {
    url_from(HELP_JSON, &lang())
}

fn lang() -> String {
    gtk::glib::language_names()
        .first()
        .map(|l| l.split(['_', '.', '@']).next().unwrap_or("en").to_owned())
        .filter(|l| l != "C" && !l.is_empty())
        .unwrap_or_else(|| "en".into())
}

fn url_from(json: &str, lang: &str) -> Option<String> {
    let h: Help = serde_json::from_str(json).ok()?;
    let path = h.topics.get("filer")?;
    let base = h.base.trim().trim_end_matches('/');
    let host_ok = base.strip_prefix("https://").is_some_and(|r| {
        !r.is_empty()
            && r.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-._~/:".contains(c))
    });
    if !host_ok {
        return None;
    }
    Some(format!("{base}/{path}?lang={lang}&from=filer"))
}

#[cfg(test)]
mod tests {
    use super::url_from;

    #[test]
    fn empty_base_gives_none() {
        assert_eq!(
            url_from(r#"{"base":"","topics":{"filer":"filer"}}"#, "pl"),
            None
        );
        assert_eq!(
            url_from(r#"{"base":"http://x","topics":{"filer":"filer"}}"#, "pl"),
            None
        );
    }

    #[test]
    fn https_base() {
        assert_eq!(
            url_from(
                r#"{"base":"https://shrimp.example/help/","topics":{"filer":"filer"}}"#,
                "pl"
            )
            .as_deref(),
            Some("https://shrimp.example/help/filer?lang=pl&from=filer")
        );
    }

    #[test]
    fn repo_file_has_topic() {
        assert!(super::HELP_JSON.contains("\"filer\""));
    }
}
