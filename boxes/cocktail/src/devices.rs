//! Lista urządzeń z `lspci -vmm -k`, `lsusb`, `/proc/cpuinfo` i sysfs, pogrupowana jak w makiecie.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub category: Category,
    pub name: String,
    /// Sterownik jądra w użyciu (z `lspci -k`), jeśli znany.
    pub driver: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    Batteries,
    Processors,
    Graphics,
    Sound,
    Cameras,
    Network,
    Bluetooth,
    Keyboards,
    Pointers,
    Disks,
    Usb,
    Biometrics,
    Other,
}

impl Category {
    pub const ALL: [Category; 13] = [
        Category::Batteries, Category::Processors, Category::Graphics, Category::Sound,
        Category::Cameras, Category::Network, Category::Bluetooth, Category::Keyboards,
        Category::Pointers, Category::Disks, Category::Usb, Category::Biometrics, Category::Other,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Category::Batteries => crate::i18n::tr("device.batteries"),
            Category::Processors => crate::i18n::tr("device.processors"),
            Category::Graphics => crate::i18n::tr("device.graphics"),
            Category::Sound => crate::i18n::tr("device.sound"),
            Category::Cameras => crate::i18n::tr("device.cameras"),
            Category::Network => crate::i18n::tr("device.network"),
            Category::Bluetooth => crate::i18n::tr("device.bluetooth"),
            Category::Keyboards => crate::i18n::tr("device.keyboards"),
            Category::Pointers => crate::i18n::tr("device.pointers"),
            Category::Disks => crate::i18n::tr("device.disks"),
            Category::Usb => crate::i18n::tr("device.usb"),
            Category::Biometrics => crate::i18n::tr("device.biometrics"),
            Category::Other => crate::i18n::tr("device.other"),
        }
    }

    /// Ikony symboliczne Adwaita, te same co w makiecie.
    pub fn icon(self) -> &'static str {
        match self {
            Category::Batteries => "battery-full-symbolic",
            Category::Processors => "system-run-symbolic",
            Category::Graphics => "video-display-symbolic",
            Category::Sound => "audio-card-symbolic",
            Category::Cameras => "camera-web-symbolic",
            Category::Network => "network-wireless-symbolic",
            Category::Bluetooth => "bluetooth-symbolic",
            Category::Keyboards => "input-keyboard-symbolic",
            Category::Pointers => "input-touchpad-symbolic",
            Category::Disks => "drive-harddisk-symbolic",
            Category::Usb => "media-flash-symbolic",
            Category::Biometrics => "security-high-symbolic",
            Category::Other => "dialog-question-symbolic",
        }
    }
}

/// Rekordy `lspci -vmm -k` (puste linie rozdzielają urządzenia).
pub fn parse_lspci(text: &str) -> Vec<Device> {
    let mut out = Vec::new();
    for block in text.split("\n\n") {
        let field = |k: &str| {
            block.lines().find_map(|l| {
                let (a, b) = l.split_once(':')?;
                (a.trim() == k).then(|| b.trim().to_string())
            })
        };
        let (Some(class), Some(device)) = (field("Class"), field("Device")) else { continue };
        let category = match class.as_str() {
            "VGA compatible controller" | "3D controller" | "Display controller" => Category::Graphics,
            "Audio device" | "Multimedia audio controller" => Category::Sound,
            "Network controller" | "Ethernet controller" => Category::Network,
            "USB controller" => Category::Usb,
            "Non-Volatile memory controller" | "SATA controller" | "RAID bus controller"
            | "Mass storage controller" | "IDE interface" => Category::Disks,
            // Elementy płyty głównej nie są tematem dla użytkownika.
            c if c.contains("bridge") || c.contains("SMBus") || c.contains("RAM memory")
                || c.contains("Signal processing") || c.contains("System peripheral")
                || c.contains("Memory controller") || c.contains("Serial bus controller")
                || c.contains("Communication controller") || c.contains("PIC") => continue,
            _ => Category::Other,
        };
        let vendor = field("Vendor").unwrap_or_default();
        let name = if vendor.is_empty() || device.starts_with(&vendor) { device } else { format!("{vendor} {device}") };
        out.push(Device { category, name, driver: field("Driver") });
    }
    out
}

/// `lsusb`: „Bus 001 Device 003: ID 0bda:58e6 Realtek Semiconductor Corp. Integrated Webcam”.
pub fn parse_lsusb(text: &str) -> Vec<Device> {
    text.lines()
        .filter_map(|l| {
            let rest = l.split_once(" ID ")?.1;
            let name = rest.split_once(' ').map(|(_, n)| n.trim()).unwrap_or("");
            let low = name.to_lowercase();
            if name.is_empty() || low.contains("hub") {
                return None;
            }
            let category = if low.contains("webcam") || low.contains("camera") {
                Category::Cameras
            } else if low.contains("bluetooth") {
                Category::Bluetooth
            } else if low.contains("keyboard") {
                Category::Keyboards
            } else if low.contains("mouse") || low.contains("touchpad") || low.contains("trackpad") {
                Category::Pointers
            } else if low.contains("fingerprint") {
                Category::Biometrics
            } else {
                Category::Usb
            };
            Some(Device { category, name: name.to_string(), driver: None })
        })
        .collect()
}

/// Nazwa procesora z `/proc/cpuinfo` (jedna, bez powtórzeń na rdzeń).
pub fn parse_cpuinfo(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix("model name")?.split_once(':').map(|(_, v)| v.trim().to_string()))
        .filter(|s| !s.is_empty())
}

/// Czytelna nazwa sterownika karty graficznej.
pub fn driver_label(driver: &str) -> String {
    match driver {
        "nvidia" => "NVIDIA".into(),
        "nouveau" => crate::i18n::tr("driver.nouveau").into(),
        "i915" | "xe" => "Intel".into(),
        "amdgpu" | "radeon" => "AMD".into(),
        other => other.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lspci() {
        let t = "Slot:\t00:00.0\nClass:\tHost bridge\nVendor:\tIntel Corporation\nDevice:\tSomething\n\n\
Slot:\t00:02.0\nClass:\tVGA compatible controller\nVendor:\tIntel Corporation\nDevice:\tAlder Lake-P GT2 [Iris Xe Graphics]\nDriver:\ti915\nModule:\ti915\n\n\
Slot:\t01:00.0\nClass:\t3D controller\nVendor:\tNVIDIA Corporation\nDevice:\tAD107M [GeForce RTX 4060 Max-Q / Mobile]\nDriver:\tnouveau\n\n\
Slot:\t02:00.0\nClass:\tNetwork controller\nVendor:\tIntel Corporation\nDevice:\tWi-Fi 6 AX201\n";
        let d = parse_lspci(t);
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].category, Category::Graphics);
        assert_eq!(d[0].driver.as_deref(), Some("i915"));
        assert_eq!(d[1].driver.as_deref(), Some("nouveau"));
        assert_eq!(d[2].name, "Intel Corporation Wi-Fi 6 AX201");
    }

    #[test]
    fn lsusb() {
        let t = "Bus 001 Device 001: ID 1d6b:0002 Linux Foundation 2.0 root hub\n\
Bus 001 Device 003: ID 0bda:58e6 Realtek Semiconductor Corp. Integrated Webcam HD\n\
Bus 001 Device 004: ID 8087:0026 Intel Corp. AX201 Bluetooth\n\
Bus 001 Device 005: ID 046d:c52b Logitech, Inc. Unifying Receiver\n";
        let d = parse_lsusb(t);
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].category, Category::Cameras);
        assert_eq!(d[1].category, Category::Bluetooth);
        assert_eq!(d[2].category, Category::Usb);
    }

    #[test]
    fn cpu() {
        let t = "processor\t: 0\nmodel name\t: 13th Gen Intel(R) Core(TM) i7-13700H\nprocessor\t: 1\n";
        assert_eq!(parse_cpuinfo(t).as_deref(), Some("13th Gen Intel(R) Core(TM) i7-13700H"));
    }
}
