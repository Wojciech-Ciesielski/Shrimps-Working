//! Start w starszej wersji systemu na próbę (systemd-boot, migawki snapper).
//!
//! „Uruchom ponownie w tej wersji” ustawia wpis migawki jako jednorazowy (`bootctl set-oneshot`): komputer
//! startuje w niej raz, a kolejny start (także po nieudanym) wraca do bieżącej wersji sam. Po zalogowaniu
//! `panko --check-boot` (autostart) pyta, czy zostawić tę wersję (`transactional-update rollback N`) czy wrócić.
//! Znacznik próby leży w katalogu domowym (poza migawkami), więc widać go po obu stronach restartu.
use std::path::PathBuf;
use std::process::Command;

/// Numer migawki, z której działa `/` (np. `/@/.snapshots/12/snapshot` → 12). Bez migawki: None.
pub fn root_snapshot() -> Option<u32> {
    root_snapshot_in(&std::fs::read_to_string("/proc/self/mountinfo").ok()?)
}

fn root_snapshot_in(mountinfo: &str) -> Option<u32> {
    mountinfo.lines().find_map(|l| {
        let f: Vec<&str> = l.split(' ').collect();
        (f.get(4) == Some(&"/")).then(|| snapshot_of(f.get(3)?))?
    })
}

/// `…/.snapshots/N/snapshot` → N.
fn snapshot_of(path: &str) -> Option<u32> {
    let rest = path.split("/.snapshots/").nth(1)?;
    let (n, tail) = rest.split_once('/')?;
    (tail.trim_end_matches('/') == "snapshot").then(|| n.parse().ok())?
}

/// Znacznik próby: numer migawki, w której komputer ma wystartować raz.
fn marker() -> PathBuf {
    gtk::glib::user_data_dir().join("panko").join("try-boot")
}

pub fn pending() -> Option<u32> {
    std::fs::read_to_string(marker()).ok()?.trim().parse().ok()
}

pub fn set_pending(num: u32) -> std::io::Result<()> {
    let m = marker();
    std::fs::create_dir_all(m.parent().unwrap())?;
    std::fs::write(m, num.to_string())
}

pub fn clear_pending() {
    let _ = std::fs::remove_file(marker());
}

#[derive(serde::Deserialize)]
struct Entry {
    id: String,
    #[serde(default)]
    options: Option<String>,
}

/// Wpis systemd-boot dla migawki z `bootctl list --json=short`.
fn entry_for(json: &str, num: u32) -> Option<String> {
    let needle = format!("/.snapshots/{num}/snapshot");
    let entries: Vec<Entry> = serde_json::from_str(json).ok()?;
    entries.into_iter().find(|e| e.options.as_deref().is_some_and(|o| o.contains(&needle))).map(|e| e.id)
}

fn bootctl_list() -> Result<String, String> {
    let out = Command::new("bootctl").args(["list", "--json=short", "--no-pager"]).output().map_err(|e| format!("bootctl: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `panko --try-boot N` (root, pkexec): wpis migawki (sdbootutil dokłada go, gdy go nie ma) jako jednorazowy.
pub fn try_boot(num: u32) -> Result<(), String> {
    let mut id = entry_for(&bootctl_list()?, num);
    if id.is_none() {
        let st = Command::new("sdbootutil").args(["add-all-kernels", &num.to_string()]).status().map_err(|e| format!("sdbootutil: {e}"))?;
        if !st.success() {
            return Err(format!("sdbootutil add-all-kernels {num}: kod {:?}", st.code()));
        }
        id = entry_for(&bootctl_list()?, num);
    }
    let id = id.ok_or_else(|| format!("brak wpisu rozruchu dla migawki {num}"))?;
    let st = Command::new("bootctl").args(["set-oneshot", &id]).status().map_err(|e| format!("bootctl: {e}"))?;
    if st.success() { Ok(()) } else { Err(format!("bootctl set-oneshot {id}: kod {:?}", st.code())) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_from_mountinfo() {
        let mi = "22 1 0:31 /@/.snapshots/12/snapshot / ro,relatime shared:1 - btrfs /dev/vda3 rw\n\
                  40 22 0:31 /@/home /home rw,relatime shared:2 - btrfs /dev/vda3 rw\n";
        assert_eq!(root_snapshot_in(mi), Some(12));
        assert_eq!(root_snapshot_in("22 1 0:31 /@ / rw - btrfs /dev/vda3 rw\n"), None);
        assert_eq!(snapshot_of("/@/.snapshots/7/snapshot/"), Some(7));
        assert_eq!(snapshot_of("/@/.snapshots/7/other"), None);
    }

    #[test]
    fn entry_lookup() {
        let j = r#"[{"id":"system-abc-6.12-3.conf","options":"root=UUID=x rootflags=subvol=@/.snapshots/3/snapshot quiet"},
                    {"id":"system-abc-6.12-12.conf","options":"rootflags=subvol=@/.snapshots/12/snapshot"},
                    {"id":"auto-reboot-to-firmware-setup"}]"#;
        assert_eq!(entry_for(j, 12).as_deref(), Some("system-abc-6.12-12.conf"));
        assert_eq!(entry_for(j, 1), None);
    }
}
