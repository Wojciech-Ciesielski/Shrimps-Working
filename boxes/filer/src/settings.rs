//! Ustawienia Filera (~/.config/shrimp/filer.json), domyślne jak w Nemo.
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// icons | list | compact
    pub view: String,
    /// 0 (najmniejsze) … 6 (największe), 3 = normalne
    pub zoom: i32,
    pub show_hidden: bool,
    pub folders_first: bool,
    /// name | size | type | modified
    pub sort: String,
    pub sort_reverse: bool,
    pub single_click: bool,
    pub confirm_trash: bool,
    pub confirm_delete: bool,
    pub thumbnails: bool,
    /// Największy plik, dla którego robimy miniaturę (MB).
    pub thumbnail_limit_mb: u64,
    pub show_menubar: bool,
    pub show_sidebar: bool,
    pub show_statusbar: bool,
    pub full_path_title: bool,
    pub show_delete_item: bool,
    pub sidebar_width: i32,
    /// Kolumny widoku listy poza nazwą: size, type, modified, accessed, owner, permissions, location
    pub columns: Vec<String>,
    pub window: (i32, i32),
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            view: "icons".into(),
            zoom: 3,
            show_hidden: false,
            folders_first: true,
            sort: "name".into(),
            sort_reverse: false,
            single_click: false,
            confirm_trash: false,
            confirm_delete: true,
            thumbnails: true,
            thumbnail_limit_mb: 50,
            show_menubar: true,
            show_sidebar: true,
            show_statusbar: true,
            full_path_title: false,
            show_delete_item: false,
            sidebar_width: 190,
            columns: vec!["size".into(), "type".into(), "modified".into()],
            window: (1000, 640),
        }
    }
}

fn path() -> PathBuf {
    gtk::glib::user_config_dir().join("shrimp").join("filer.json")
}

thread_local! {
    static S: RefCell<Settings> = RefCell::new(
        std::fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    );
}

pub fn get() -> Settings {
    S.with(|s| s.borrow().clone())
}

pub fn update(f: impl FnOnce(&mut Settings)) {
    S.with(|s| {
        let mut s = s.borrow_mut();
        f(&mut s);
        let p = path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, serde_json::to_string_pretty(&*s).unwrap_or_default());
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn old_file_gets_defaults() {
        let s: super::Settings = serde_json::from_str(r#"{"view":"list"}"#).unwrap();
        assert_eq!(s.view, "list");
        assert!(s.folders_first);
        assert_eq!(s.zoom, 3);
    }
}
