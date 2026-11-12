//! Wybrane pakiety systemu pokazane w zakładce Pakiety (kategorie z makiety `cocktail.html`).
//! Nazwy pakietów to nazwy z repozytoriów openSUSE Leap; sprawdzić na Leap 16.1 przed wydaniem.

pub struct Item {
    /// Klucz tłumaczenia (`pkg.*`) albo nazwa własna (VLC, gcc); wyświetlać przez `crate::i18n::tr`.
    pub title: &'static str,
    pub packages: &'static [&'static str],
}

pub struct Section {
    /// Klucz tłumaczenia (`cat.*`).
    pub title: &'static str,
    pub icon: &'static str,
    pub items: &'static [Item],
}

const fn i(title: &'static str, packages: &'static [&'static str]) -> Item {
    Item { title, packages }
}

pub const SECTIONS: &[Section] = &[
    Section {
        title: "cat.multimedia",
        icon: "applications-multimedia-symbolic",
        items: &[
            // Te same pakiety co w kroku Setup „Obraz i dźwięk” (boxes/linu-x: /usr/share/shrimp/linux/drivers.conf), bez Packmana.
            i("pkg.video-codecs", &["gstreamer-plugins-good", "gstreamer-plugins-bad", "gstreamer-1.20-plugin-openh264", "mozilla-openh264"]),
            i("pkg.audio-codecs", &["gstreamer-plugins-base", "gstreamer-plugins-ugly"]),
            i("VLC", &["vlc"]),
        ],
    },
    Section {
        title: "cat.fonts",
        icon: "font-x-generic-symbolic",
        items: &[
            i("pkg.ms-fonts", &["fetchmsttfonts"]),
            i("Noto", &["noto-sans-fonts"]),
            i("Noto Color Emoji", &["google-noto-coloremoji-fonts"]),
        ],
    },
    Section {
        title: "cat.runtimes",
        icon: "application-x-executable-symbolic",
        items: &[
            i("Java (OpenJDK 21)", &["java-21-openjdk"]),
            i("Python 3", &["python3"]),
            i("Node.js", &["nodejs-default"]),
        ],
    },
    Section {
        title: "cat.dev",
        icon: "applications-engineering-symbolic",
        items: &[
            i("gcc", &["gcc"]),
            i("make", &["make"]),
            i("cmake", &["cmake"]),
            i("git", &["git"]),
            i("autoconf", &["autoconf"]),
            i("automake", &["automake"]),
            i("pkg-config", &["pkgconf-pkg-config"]),
            i("flex", &["flex"]),
            i("bison", &["bison"]),
            i("ninja", &["ninja"]),
            i("meson", &["meson"]),
            i("tk", &["tk"]),
        ],
    },
    Section {
        title: "cat.network",
        icon: "network-wired-symbolic",
        items: &[
            i("nmap", &["nmap"]),
            i("OpenSSH", &["openssh"]),
            i("WireGuard", &["wireguard-tools"]),
            i("pkg.samba", &["samba"]),
            i("pkg.avahi", &["avahi"]),
        ],
    },
    Section {
        title: "cat.printing",
        icon: "printer-symbolic",
        items: &[i("CUPS", &["cups"]), i("cups-filters", &["cups-filters"]), i("Gutenprint", &["gutenprint"])],
    },
    Section {
        title: "cat.firmware",
        icon: "media-flash-symbolic",
        items: &[i("fwupd", &["fwupd"])],
    },
    Section {
        title: "cat.security",
        icon: "security-high-symbolic",
        items: &[i("fprintd", &["fprintd"]), i("cryptsetup", &["cryptsetup"]), i("mokutil", &["mokutil"])],
    },
    Section {
        title: "cat.virtualization",
        icon: "computer-symbolic",
        items: &[i("QEMU", &["qemu"]), i("KVM", &["qemu-kvm"]), i("virt-manager", &["virt-manager"])],
    },
    Section {
        title: "cat.system",
        icon: "applications-system-symbolic",
        items: &[
            i("htop", &["htop"]),
            i("smartmontools", &["smartmontools"]),
            i("rsync", &["rsync"]),
            i("gnome-disk-utility", &["gnome-disk-utility"]),
            i("pkg.cockpit", &["cockpit"]),
        ],
    },
];

pub fn all_packages() -> Vec<&'static str> {
    let mut v: Vec<&str> = SECTIONS.iter().flat_map(|s| s.items.iter().flat_map(|i| i.packages.iter().copied())).collect();
    v.sort_unstable();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_are_valid() {
        for p in super::all_packages() {
            assert!(crate::zypper::valid_name(p), "{p}");
            assert!(!crate::zypper::is_protected(p), "{p}");
        }
    }
}
