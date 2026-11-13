//! Parsery wyjścia zyppera (`--xmlout`) i pomocnicze funkcje bez GTK, żeby dało się je testować.

/// Pakiet z wyniku `zypper --xmlout search`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub name: String,
    pub summary: String,
    /// `installed`, `not-installed` albo `other-version`.
    pub installed: bool,
}

/// Pakiet z wyniku `zypper --xmlout list-updates`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    pub name: String,
    pub old: String,
    pub new: String,
}

/// Komunikaty błędów, które zypper zapisuje w XML jako `<message type="error">`.
pub fn errors(xml: &str) -> Vec<String> {
    let Ok(doc) = roxmltree::Document::parse(xml) else { return Vec::new() };
    doc.descendants()
        .filter(|n| n.has_tag_name("message") && n.attribute("type") == Some("error"))
        .filter_map(|n| n.text().map(|t| t.trim().to_string()))
        .filter(|t| !t.is_empty())
        .collect()
}

pub fn parse_search(xml: &str) -> Result<Vec<Found>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let mut out: Vec<Found> = Vec::new();
    for n in doc.descendants().filter(|n| n.has_tag_name("solvable")) {
        if n.attribute("kind").unwrap_or("package") != "package" {
            continue;
        }
        let Some(name) = n.attribute("name") else { continue };
        // Ten sam pakiet bywa w kilku repozytoriach; zostaje jeden wiersz.
        if let Some(f) = out.iter_mut().find(|f| f.name == name) {
            f.installed |= n.attribute("status") == Some("installed");
            continue;
        }
        out.push(Found {
            name: name.to_string(),
            summary: n.attribute("summary").unwrap_or("").to_string(),
            installed: n.attribute("status") == Some("installed"),
        });
    }
    Ok(out)
}

pub fn parse_updates(xml: &str) -> Result<Vec<Update>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let mut out: Vec<Update> = doc
        .descendants()
        .filter(|n| n.has_tag_name("update") && n.attribute("kind").unwrap_or("package") == "package")
        .filter_map(|n| {
            Some(Update {
                name: n.attribute("name")?.to_string(),
                old: n.attribute("edition-old").unwrap_or("").to_string(),
                new: n.attribute("edition").unwrap_or("").to_string(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    Ok(out)
}

/// Nazwy pakietów, które usunąłby `zypper remove --dry-run` (sekcja `<to-remove>`).
pub fn parse_remove_preview(xml: &str) -> Result<Vec<String>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let mut out: Vec<String> = doc
        .descendants()
        .filter(|n| n.has_tag_name("to-remove"))
        .flat_map(|n| n.children().filter(|c| c.has_tag_name("solvable")))
        .filter_map(|n| n.attribute("name").map(str::to_string))
        .collect();
    out.sort();
    out.dedup();
    Ok(out)
}

/// Pakiety, bez których system nie wstanie. Ta sama lista jest w `cocktail-helper`.
pub const PROTECTED: &[&str] = &[
    "kernel-default", "glibc", "systemd", "zypper", "libzypp", "rpm", "transactional-update",
    "snapper", "btrfs-progs", "grub2", "shim", "dracut", "gnome-shell", "gdm", "mutter",
    "NetworkManager", "polkit", "sudo", "bash", "coreutils", "filesystem", "cocktail",
    "shrimp-release",
];

pub fn is_protected(name: &str) -> bool {
    PROTECTED.contains(&name)
}

/// Nazwa pakietu bezpieczna do przekazania pomocnikowi (to samo sprawdza `cocktail-helper`).
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || "+._-".contains(c))
}

/// Pakiet sterownika (ta sama lista co `driver_updates` w `cocktail-helper`): jądro, firmware,
/// mikrokod, NVIDIA, moduły KMP, fwupd.
pub fn is_driver(name: &str) -> bool {
    name == "kernel-default"
        || name.starts_with("kernel-firmware")
        || name.starts_with("ucode-")
        || name.starts_with("nvidia-")
        || name.starts_with("libnvidia")
        || name.starts_with("fwupd")
        || name.contains("-kmp-")
}

/// Liczba urządzeń z nowszym oprogramowaniem układowym w `fwupdmgr get-updates --json`.
pub fn fwupd_updates(json: &str) -> usize {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else { return 0 };
    v.get("Devices")
        .and_then(|d| d.as_array())
        .map(|d| d.iter().filter(|x| x.get("Releases").and_then(|r| r.as_array()).is_some_and(|r| !r.is_empty())).count())
        .unwrap_or(0)
}

/// Nazwa systemu do wyświetlenia z `/etc/os-release`, np. „Shrimp 26”.
pub fn os_name(os_release: &str) -> String {
    let get = |key: &str| {
        os_release.lines().find_map(|l| {
            let v = l.strip_prefix(key)?.strip_prefix('=')?;
            Some(v.trim().trim_matches('"').to_string())
        })
    };
    match (get("NAME"), get("VERSION_ID"), get("PRETTY_NAME")) {
        (Some(n), Some(v), _) if n == "Shrimp" => format!("{n} {v}"),
        (_, _, Some(p)) => p,
        (Some(n), _, _) => n,
        _ => "Linux".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search() {
        let xml = r#"<?xml version='1.0'?>
<stream>
<search-result version="0.0">
<solvable-list>
<solvable status="installed" name="vlc" summary="Graphical media player" kind="package"/>
<solvable status="not-installed" name="vlc" summary="Graphical media player" kind="package"/>
<solvable status="not-installed" name="vlc-codecs" summary="Additional codecs" kind="package"/>
<solvable status="not-installed" name="vlc" summary="pattern" kind="pattern"/>
</solvable-list>
</search-result>
</stream>"#;
        let r = parse_search(xml).unwrap();
        assert_eq!(r.len(), 2);
        assert!(r[0].installed);
        assert_eq!(r[1].name, "vlc-codecs");
        assert!(!r[1].installed);
    }

    #[test]
    fn updates() {
        let xml = r#"<?xml version='1.0'?>
<stream>
<message type="warning">Repository 'oss' is out-of-date.</message>
<update-status version="0.6">
<update-list>
<update kind="package" name="kernel-default" edition="6.12.2-1.1" arch="x86_64" edition-old="6.12.1-1.1"><summary>Kernel</summary></update>
<update kind="package" name="Mesa" edition="25.2.2-1.1" arch="x86_64" edition-old="25.2.1-1.1"><summary>Mesa</summary></update>
<update kind="patch" name="openSUSE-2026-1" edition="1" arch="noarch"><summary>p</summary></update>
</update-list>
</update-status>
</stream>"#;
        let u = parse_updates(xml).unwrap();
        assert_eq!(u.len(), 2);
        assert_eq!(u[0].name, "Mesa");
        assert_eq!(u[1].old, "6.12.1-1.1");
        assert_eq!(u[1].new, "6.12.2-1.1");
    }

    #[test]
    fn remove_preview_and_errors() {
        let xml = r#"<?xml version='1.0'?>
<stream>
<install-summary download-size="0" space-usage-diff="-100" packages-to-change="2">
<to-remove>
<solvable type="package" name="vlc" edition="3.0.21" arch="x86_64" summary="x"/>
<solvable type="package" name="vlc-qt" edition="3.0.21" arch="x86_64" summary="y"/>
</to-remove>
</install-summary>
</stream>"#;
        assert_eq!(parse_remove_preview(xml).unwrap(), vec!["vlc", "vlc-qt"]);
        let err = r#"<stream><message type="error">No provider of 'xyz' found.</message></stream>"#;
        assert_eq!(errors(err), vec!["No provider of 'xyz' found."]);
        assert!(parse_remove_preview(err).unwrap().is_empty());
    }

    #[test]
    fn names() {
        assert!(valid_name("gstreamer-1.20-plugin-openh264"));
        assert!(valid_name("libstdc++6"));
        assert!(!valid_name("-rf"));
        assert!(!valid_name("a b"));
        assert!(!valid_name("a;b"));
        assert!(!valid_name(""));
        assert!(is_protected("glibc"));
        assert!(!is_protected("vlc"));
    }

    #[test]
    fn polish_plural() {
        let p = |n| crate::i18n::trn("updates.packages", n, &[]);
        assert_eq!(p(1), "1 pakiet");
        assert_eq!(p(3), "3 pakiety");
        assert_eq!(p(5), "5 pakietów");
        assert_eq!(p(12), "12 pakietów");
        assert_eq!(p(22), "22 pakiety");
        assert_eq!(p(0), "0 pakietów");
    }

    #[test]
    fn drivers() {
        assert!(is_driver("kernel-firmware-iwlwifi"));
        assert!(is_driver("nvidia-open-driver-G06-signed-kmp-default"));
        assert!(is_driver("ucode-intel"));
        assert!(!is_driver("vlc"));
        assert_eq!(fwupd_updates(r#"{"Devices":[{"Name":"BIOS","Releases":[{"Version":"2"}]},{"Name":"SSD","Releases":[]}]}"#), 1);
        assert_eq!(fwupd_updates("No updatable devices"), 0);
    }

    #[test]
    fn os() {
        assert_eq!(os_name("NAME=\"Shrimp\"\nVERSION_ID=\"26\"\nPRETTY_NAME=\"Shrimp 26 Mont Blanc\"\n"), "Shrimp 26");
        assert_eq!(os_name("NAME=\"openSUSE Leap\"\nVERSION_ID=\"16.1\"\nPRETTY_NAME=\"openSUSE Leap 16.1\"\n"), "openSUSE Leap 16.1");
    }
}
