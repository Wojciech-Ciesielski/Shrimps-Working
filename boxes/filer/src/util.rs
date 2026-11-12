//! Drobiazgi: rozmiary, daty, ikony, nazwy, ścieżki (jak w Nemo).
use crate::i18n::{tr, trn};
use gtk::{gio, glib, prelude::*};

/// Atrybuty plików czytane dla widoków (DirectoryList, wyszukiwanie, właściwości).
pub const ATTRS: &str = "standard::*,time::modified,time::access,thumbnail::path,thumbnail::failed,access::*,unix::mode,owner::user,owner::group,trash::orig-path,trash::deletion-date,filesystem::readonly";

/// Rozmiar jak w Nemo (jednostki dziesiętne): „1,5 MB”, „512 bajtów”.
pub fn size(n: u64) -> String {
    if n < 1000 {
        return trn("size.bytes", n, &[]);
    }
    let units = ["kB", "MB", "GB", "TB", "PB"];
    let mut v = n as f64 / 1000.0;
    let mut u = 0;
    while v >= 1000.0 && u < units.len() - 1 {
        v /= 1000.0;
        u += 1;
    }
    let s = if v < 10.0 { format!("{v:.1}") } else { format!("{v:.0}") };
    let dec = tr("size.decimal");
    format!("{} {}", s.replace('.', dec), units[u])
}

/// Data modyfikacji: „dziś 14:03”, „wczoraj 09:10”, inaczej „12.11.2026 14:03”.
pub fn date(t: &glib::DateTime) -> String {
    let local = t.to_local().unwrap_or_else(|_| t.clone());
    let now = glib::DateTime::now_local().unwrap();
    let day = |d: &glib::DateTime| (d.year(), d.day_of_year());
    let hm = local.format("%H:%M").map(|s| s.to_string()).unwrap_or_default();
    if day(&local) == day(&now) {
        format!("{} {hm}", tr("date.today"))
    } else if day(&local) == day(&now.add_days(-1).unwrap()) {
        format!("{} {hm}", tr("date.yesterday"))
    } else {
        local.format(tr("date.format")).map(|s| s.to_string()).unwrap_or_default()
    }
}

pub fn file_of(info: &gio::FileInfo) -> Option<gio::File> {
    info.attribute_object("standard::file").and_downcast::<gio::File>()
}

pub fn access_time(info: &gio::FileInfo) -> Option<glib::DateTime> {
    let t = info.attribute_uint64("time::access");
    if t == 0 { None } else { glib::DateTime::from_unix_local(t as i64).ok() }
}

pub fn is_dir(info: &gio::FileInfo) -> bool {
    matches!(info.file_type(), gio::FileType::Directory | gio::FileType::Mountable)
}

pub fn display_name(f: &gio::File) -> String {
    if f.uri_scheme().as_deref() == Some("trash") && f.path().is_none() && f.uri().trim_end_matches('/') == "trash:" {
        return tr("place.trash").into();
    }
    if let Some(p) = f.path() {
        if p == glib::home_dir() {
            return tr("place.home").into();
        }
        if p.as_os_str() == "/" {
            return tr("place.filesystem").into();
        }
    }
    match f.uri().trim_end_matches('/') {
        "trash:" => return tr("place.trash").into(),
        "recent:" => return tr("place.recent").into(),
        "network:" => return tr("place.network").into(),
        "computer:" => return tr("place.computer").into(),
        _ => {}
    }
    f.query_info("standard::display-name", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE)
        .map(|i| i.display_name().to_string())
        .unwrap_or_else(|_| f.basename().map(|b| b.to_string_lossy().into_owned()).unwrap_or_else(|| f.uri().to_string()))
}

/// Ikona folderu dla miejsc (Dom, Dokumenty…), jak w panelu bocznym.
pub fn place_icon(f: &gio::File) -> &'static str {
    let Some(p) = f.path() else {
        return match f.uri_scheme().as_deref() {
            Some("trash") => "user-trash-symbolic",
            Some("recent") => "document-open-recent-symbolic",
            Some("network") => "network-workgroup-symbolic",
            _ => "folder-remote-symbolic",
        };
    };
    if p == glib::home_dir() {
        return "user-home-symbolic";
    }
    if p.as_os_str() == "/" {
        return "drive-harddisk-symbolic";
    }
    let special = [
        (glib::UserDirectory::Desktop, "user-desktop-symbolic"),
        (glib::UserDirectory::Documents, "folder-documents-symbolic"),
        (glib::UserDirectory::Downloads, "folder-download-symbolic"),
        (glib::UserDirectory::Music, "folder-music-symbolic"),
        (glib::UserDirectory::Pictures, "folder-pictures-symbolic"),
        (glib::UserDirectory::Videos, "folder-videos-symbolic"),
        (glib::UserDirectory::Templates, "folder-templates-symbolic"),
        (glib::UserDirectory::PublicShare, "folder-publicshare-symbolic"),
    ];
    for (d, icon) in special {
        if glib::user_special_dir(d).as_deref() == Some(p.as_path()) {
            return icon;
        }
    }
    "folder-symbolic"
}

/// Wolna nazwa w folderze: „Nowy folder”, „Nowy folder 2”, … albo „plik (kopia).txt”.
pub fn free_name(dir: &gio::File, base: &str, copy: bool) -> String {
    let (stem, ext) = match base.rfind('.') {
        Some(i) if i > 0 && !base[..i].is_empty() => (&base[..i], &base[i..]),
        _ => (base, ""),
    };
    let candidate = |n: u32| -> String {
        if copy {
            if n == 1 { format!("{stem} ({}){ext}", tr("copy.suffix")) } else { format!("{stem} ({} {n}){ext}", tr("copy.suffix")) }
        } else if n == 1 {
            base.to_string()
        } else {
            format!("{stem} {n}{ext}")
        }
    };
    (1..10_000).map(candidate).find(|n| !dir.child(n).query_exists(gio::Cancellable::NONE)).unwrap_or_else(|| base.to_string())
}

/// Tekst z paska adresu: ścieżka, „~/…” albo adres (smb://, sftp://…).
pub fn parse_location(s: &str) -> gio::File {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('~') {
        return gio::File::for_path(glib::home_dir().join(rest.trim_start_matches('/')));
    }
    gio::File::for_commandline_arg(s)
}

/// Uprawnienia jak w `ls -l`: „drwxr-xr-x”.
pub fn mode_string(mode: u32) -> String {
    let kind = match mode & 0o170000 {
        0o040000 => 'd',
        0o120000 => 'l',
        _ => '-',
    };
    let mut s = String::from(kind);
    for shift in [6, 3, 0] {
        let b = (mode >> shift) & 7;
        s.push(if b & 4 != 0 { 'r' } else { '-' });
        s.push(if b & 2 != 0 { 'w' } else { '-' });
        s.push(if b & 1 != 0 { 'x' } else { '-' });
    }
    if mode == 0 { String::new() } else { s }
}

/// Bezpieczna nazwa pliku: bez „/” i bez pustej.
pub fn valid_name(n: &str) -> bool {
    !n.trim().is_empty() && !n.contains('/') && n != "." && n != ".."
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sizes() {
        assert_eq!(size(1), "1 bajt");
        assert_eq!(size(512), "512 bajtów");
        assert_eq!(size(1500), "1,5 kB");
        assert_eq!(size(23_000_000), "23 MB");
        assert_eq!(size(4_700_000_000), "4,7 GB");
    }
    #[test]
    fn modes() {
        assert_eq!(mode_string(0o040755), "drwxr-xr-x");
        assert_eq!(mode_string(0o100644), "-rw-r--r--");
    }
    #[test]
    fn names() {
        assert!(valid_name("raport.txt"));
        assert!(!valid_name("a/b"));
        assert!(!valid_name("  "));
        let dir = gio::File::for_path(std::env::temp_dir().join(format!("filer-test-{}", std::process::id())));
        let _ = dir.make_directory(gio::Cancellable::NONE);
        assert_eq!(free_name(&dir, "Nowy folder", false), "Nowy folder");
        dir.child("Nowy folder").make_directory(gio::Cancellable::NONE).unwrap();
        assert_eq!(free_name(&dir, "Nowy folder", false), "Nowy folder 2");
        assert_eq!(free_name(&dir, "raport.txt", true), "raport (kopia).txt");
        let _ = dir.child("Nowy folder").delete(gio::Cancellable::NONE);
        let _ = dir.delete(gio::Cancellable::NONE);
    }
}
