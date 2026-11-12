//! Kategorie z paska bocznego i ich mapowanie na kategorie freedesktop.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Graphics,
    Video,
    Music,
    Games,
    Office,
    Internet,
    Development,
    Tools,
}

impl Category {
    pub const ALL: [Category; 8] = [
        Category::Graphics,
        Category::Video,
        Category::Music,
        Category::Games,
        Category::Office,
        Category::Internet,
        Category::Development,
        Category::Tools,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Category::Graphics => "graphics",
            Category::Video => "video",
            Category::Music => "music",
            Category::Games => "games",
            Category::Office => "office",
            Category::Internet => "internet",
            Category::Development => "development",
            Category::Tools => "tools",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Category::Graphics => crate::i18n::tr("category.graphics"),
            Category::Video => crate::i18n::tr("category.video"),
            Category::Music => crate::i18n::tr("category.music"),
            Category::Games => crate::i18n::tr("category.games"),
            Category::Office => crate::i18n::tr("category.office"),
            Category::Internet => crate::i18n::tr("category.internet"),
            Category::Development => crate::i18n::tr("category.development"),
            Category::Tools => crate::i18n::tr("category.tools"),
        }
    }

    /// Ikony symboliczne Adwaita (jak w makiecie).
    pub fn icon(self) -> &'static str {
        match self {
            Category::Graphics => "applications-graphics-symbolic",
            Category::Video => "camera-video-symbolic",
            Category::Music => "audio-x-generic-symbolic",
            Category::Games => "input-gaming-symbolic",
            Category::Office => "x-office-document-symbolic",
            Category::Internet => "user-available-symbolic",
            Category::Development => "utilities-terminal-symbolic",
            Category::Tools => "applications-utilities-symbolic",
        }
    }

    /// Pierwsza pasująca kategoria; kolejność rozstrzyga aplikacje z kilkoma kategoriami
    /// (np. „AudioVideo;Video” → Wideo, „Game;Graphics” → Gry).
    pub fn for_freedesktop(cats: &[String]) -> Option<Category> {
        let has = |n: &str| cats.iter().any(|c| c == n);
        if has("Game") {
            Some(Category::Games)
        } else if has("Video") || has("TV") || has("Player") && !has("Audio") {
            Some(Category::Video)
        } else if has("Audio") || has("Music") {
            Some(Category::Music)
        } else if has("AudioVideo") {
            Some(Category::Video)
        } else if has("Graphics") || has("Photography") {
            Some(Category::Graphics)
        } else if has("Development") || has("IDE") {
            Some(Category::Development)
        } else if has("Office") || has("Education") || has("Science") {
            Some(Category::Office)
        } else if has("Network") || has("WebBrowser") || has("Chat") || has("InstantMessaging") || has("Email") {
            Some(Category::Internet)
        } else if has("Utility") || has("System") || has("Settings") {
            Some(Category::Tools)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn v(s: &str) -> Vec<String> {
        s.split(';').map(String::from).collect()
    }
    #[test]
    fn mapping() {
        assert_eq!(Category::for_freedesktop(&v("AudioVideo;Video;Player")), Some(Category::Video));
        assert_eq!(Category::for_freedesktop(&v("AudioVideo;Audio;Player")), Some(Category::Music));
        assert_eq!(Category::for_freedesktop(&v("Game;ArcadeGame")), Some(Category::Games));
        assert_eq!(Category::for_freedesktop(&v("Network;WebBrowser")), Some(Category::Internet));
        assert_eq!(Category::for_freedesktop(&v("GTK;Utility")), Some(Category::Tools));
        assert_eq!(Category::for_freedesktop(&v("Graphics;2DGraphics")), Some(Category::Graphics));
        assert_eq!(Category::for_freedesktop(&v("Other")), None);
    }
}
