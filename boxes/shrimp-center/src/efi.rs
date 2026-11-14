//! Wpisy rozruchowe UEFI (efibootmgr): który system uruchamia się domyślnie (BootOrder) i który
//! tylko przy następnym starcie (BootNext). Zastępuje dawny wpis Windows w menu GRUB-a.

use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Shrimp,
    Windows,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Cztery cyfry szesnastkowe, np. "0001".
    pub id: String,
    pub name: String,
    pub kind: Kind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub current: Option<String>,
    pub next: Option<String>,
    pub order: Vec<String>,
    /// Tylko systemy (wpisy z plikiem na dysku), bez sieci, pendrive'ów i zapasowego \EFI\BOOT.
    pub entries: Vec<Entry>,
}

impl State {
    /// Domyślny system: pierwszy system z BootOrder.
    pub fn default_entry(&self) -> Option<&Entry> {
        self.order
            .iter()
            .find_map(|id| self.entries.iter().find(|e| &e.id == id))
            .or_else(|| self.entries.first())
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// BootOrder z `id` na początku (reszta w tej samej kolejności).
    pub fn order_with_first(&self, id: &str) -> Vec<String> {
        let mut v = vec![id.to_owned()];
        v.extend(self.order.iter().filter(|x| *x != id).cloned());
        v
    }
}

pub fn valid_id(id: &str) -> bool {
    id.len() == 4 && id.chars().all(|c| c.is_ascii_hexdigit())
}

/// Komputer uruchomiony w trybie UEFI (zmienne EFI dostępne).
pub fn available() -> bool {
    std::path::Path::new("/sys/firmware/efi/efivars").is_dir()
}

fn kind_of(name: &str, path: &str) -> Kind {
    let n = name.to_ascii_lowercase();
    let p = path.to_ascii_lowercase().replace('/', "\\");
    if n.contains("windows") || p.contains("\\efi\\microsoft\\") {
        Kind::Windows
    } else if n.contains("shrimp")
        || n.contains("linux boot manager")
        || n.contains("opensuse")
        || p.contains("\\efi\\systemd\\")
        || p.contains("\\efi\\opensuse\\")
        || p.contains("\\efi\\shrimp\\")
    {
        Kind::Shrimp
    } else {
        Kind::Other
    }
}

/// Wyjście `efibootmgr -v` (wersje 17 i 18: nazwa i ścieżka rozdzielone tabulatorem).
pub fn parse(out: &str) -> State {
    let mut st = State::default();
    for line in out.lines() {
        let line = line.trim_end();
        if let Some(v) = line.strip_prefix("BootCurrent:") {
            st.current = Some(v.trim().to_uppercase());
        } else if let Some(v) = line.strip_prefix("BootNext:") {
            st.next = Some(v.trim().to_uppercase());
        } else if let Some(v) = line.strip_prefix("BootOrder:") {
            st.order = v
                .split(',')
                .map(|s| s.trim().to_uppercase())
                .filter(|s| valid_id(s))
                .collect();
        } else if let Some(rest) = line.strip_prefix("Boot") {
            let (Some(id), Some(tail)) = (rest.get(..4), rest.get(4..)) else {
                continue;
            };
            // Tylko aktywne wpisy („Boot0001*”).
            let Some(tail) = tail.strip_prefix('*') else {
                continue;
            };
            if !valid_id(id) {
                continue;
            }
            let (name, path) = match tail.split_once('\t') {
                Some((n, p)) => (n.trim(), p.trim()),
                None => (tail.trim(), ""),
            };
            let kind = kind_of(name, path);
            let lp = path.to_ascii_lowercase().replace('/', "\\");
            let is_file = lp.contains("file(\\efi\\") && !lp.contains("\\efi\\boot\\boot");
            // Bez ścieżki (efibootmgr bez -v) zostają tylko znane systemy.
            if kind == Kind::Other && !is_file {
                continue;
            }
            st.entries.push(Entry {
                id: id.to_uppercase(),
                name: name.to_owned(),
                kind,
            });
        }
    }
    // Wpis, z którego uruchomiono ten system, to Shrimp, nawet z nieznaną nazwą.
    if let Some(cur) = st.current.clone() {
        if let Some(e) = st.entries.iter_mut().find(|e| e.id == cur && e.kind == Kind::Other) {
            e.kind = Kind::Shrimp;
        }
    }
    st
}

fn efibootmgr(args: &[&str]) -> Result<String, String> {
    let out = Command::new("efibootmgr")
        .args(args)
        .output()
        .map_err(|e| format!("efibootmgr: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn read() -> Result<State, String> {
    efibootmgr(&["-v"]).map(|o| parse(&o))
}

/// `shrimp-center --boot-next ID` (root, przez pkexec): ten system tylko przy następnym starcie.
pub fn set_boot_next(id: &str) -> Result<(), String> {
    checked(id)?;
    efibootmgr(&["-q", "-n", id]).map(|_| ())
}

/// `shrimp-center --default ID` (root, przez pkexec): system na początku BootOrder.
pub fn set_default(id: &str) -> Result<(), String> {
    let st = checked(id)?;
    let order = st.order_with_first(id).join(",");
    efibootmgr(&["-q", "-o", &order]).map(|_| ())
}

/// Tylko ID istniejącego systemu z listy; nic innego nie trafia do efibootmgr.
fn checked(id: &str) -> Result<State, String> {
    if !valid_id(id) {
        return Err(format!("niepoprawny wpis {id:?}"));
    }
    let st = read()?;
    if st.get(id).is_none() {
        return Err(format!("brak systemu {id} na liście UEFI"));
    }
    Ok(st)
}

/// Dane do `--demo`: Shrimp i Windows na jednym dysku.
pub fn demo() -> State {
    parse(DEMO)
}

const DEMO: &str = "BootCurrent: 0002\nTimeout: 0 seconds\nBootOrder: 0002,0001,0000,0003\n\
Boot0000* UEFI: PXE IPv4 Intel(R) Ethernet\tPciRoot(0x0)/Pci(0x1c,0x0)/MAC(000000000000,0)/IPv4(0.0.0.0)\n\
Boot0001* Windows Boot Manager\tHD(1,GPT,6c1a2b3c-0000-0000-0000-000000000001,0x800,0x82000)/File(\\EFI\\Microsoft\\Boot\\bootmgfw.efi)\n\
Boot0002* Linux Boot Manager\tHD(1,GPT,6c1a2b3c-0000-0000-0000-000000000001,0x800,0x82000)/File(\\EFI\\systemd\\systemd-bootx64.efi)\n\
Boot0003* UEFI: SanDisk USB\tPciRoot(0x0)/Pci(0x14,0x0)/USB(1,0)/HD(1,MBR,0x0,0x800,0x1000)\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_demo() {
        let st = demo();
        assert_eq!(st.current.as_deref(), Some("0002"));
        assert_eq!(st.order, ["0002", "0001", "0000", "0003"]);
        let ids: Vec<_> = st.entries.iter().map(|e| (e.id.as_str(), e.kind)).collect();
        assert_eq!(ids, [("0001", Kind::Windows), ("0002", Kind::Shrimp)]);
        assert_eq!(st.default_entry().unwrap().kind, Kind::Shrimp);
        assert_eq!(st.order_with_first("0001"), ["0001", "0002", "0000", "0003"]);
    }

    #[test]
    fn without_paths_and_inactive() {
        let st = parse("BootCurrent: 0005\nBootNext: 0001\nBootOrder: 0001,0005\nBoot0001* Windows Boot Manager\nBoot0004  Shrimp\nBoot0005* opensuse-secureboot\nBoot0006* Hard Drive\n");
        assert_eq!(st.next.as_deref(), Some("0001"));
        let ids: Vec<_> = st.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["0001", "0005"]);
        assert_eq!(st.default_entry().unwrap().kind, Kind::Windows);
    }

    #[test]
    fn current_unknown_name_is_shrimp() {
        let st = parse("BootCurrent: 0007\nBootOrder: 0007\nBoot0007* My Disk\tHD(1,GPT,x)/File(\\EFI\\custom\\grubx64.efi)\n");
        assert_eq!(st.entries[0].kind, Kind::Shrimp);
    }

    #[test]
    fn ids() {
        assert!(valid_id("00AF"));
        assert!(!valid_id("1"));
        assert!(!valid_id("0001,0002"));
        assert!(!valid_id("-o 1"));
    }
}
