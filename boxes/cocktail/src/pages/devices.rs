//! Urządzenia: sprzęt w kategoriach z makiety i sterownik karty graficznej w użyciu.

use gtk::glib;
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::app;
use crate::devices::{self, Category, Device};
use crate::tree::Tree;

pub struct Devices {
    pub widget: gtk::ScrolledWindow,
    tree: Rc<Tree>,
    none: gtk::Label,
    status: gtk::Label,
    loaded: Cell<bool>,
}

impl Devices {
    pub fn new() -> Rc<Self> {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let tree = Tree::new();
        let none = super::no_results();
        let status = super::note(crate::i18n::tr("state.loading"));
        content.append(&status);
        content.append(&tree.list);
        content.append(&none);
        Rc::new(Devices { widget: super::scrolled(&content), tree, none, status, loaded: Cell::new(false) })
    }

    pub fn load(self: &Rc<Self>) {
        if self.loaded.replace(true) {
            return;
        }
        let p = self.clone();
        glib::spawn_future_local(async move {
            let list = collect().await;
            p.render(&list);
        });
    }

    fn render(&self, list: &[Device]) {
        self.status.set_visible(list.is_empty());
        self.status.set_text(crate::i18n::tr("devices.error"));
        let host = glib::host_name();
        let root = self.tree.group(None, "computer-symbolic", &host, true);
        for c in Category::ALL {
            let items: Vec<&Device> = list.iter().filter(|d| d.category == c).collect();
            if items.is_empty() {
                continue;
            }
            let g = self.tree.group(Some(root), c.icon(), c.title(), c == Category::Graphics);
            for d in items {
                self.tree.leaf(Some(g), None, &d.name, None, None);
            }
            if c == Category::Graphics {
                let mut drivers: Vec<String> = list
                    .iter()
                    .filter(|d| d.category == Category::Graphics)
                    .filter_map(|d| d.driver.as_deref().map(devices::driver_label))
                    .collect();
                drivers.dedup();
                if !drivers.is_empty() {
                    let s = crate::i18n::trn("devices.drivers", drivers.len() as u64, &[("list", &drivers.join(", "))]);
                    self.tree.leaf(Some(g), None, &s, None, None);
                }
            }
        }
        self.tree.refresh();
    }

    pub fn search(&self, q: &str) {
        let n = self.tree.set_filter(q);
        self.none.set_visible(!q.trim().is_empty() && n == 0);
    }
}

async fn collect() -> Vec<Device> {
    let mut list = Vec::new();
    if let Some(cpu) = std::fs::read_to_string("/proc/cpuinfo").ok().as_deref().and_then(devices::parse_cpuinfo) {
        list.push(Device { category: Category::Processors, name: cpu, driver: None });
    }
    for (dir, name) in sys_entries("/sys/class/power_supply") {
        if read(&dir, "type").as_deref() == Some("Battery") {
            let model = read(&dir, "model_name").filter(|m| !m.is_empty()).unwrap_or(name);
            list.push(Device { category: Category::Batteries, name: model, driver: None });
        }
    }
    let pci = app::run(&["lspci", "-vmm", "-k"]).await;
    list.extend(devices::parse_lspci(&pci.stdout));
    let usb = app::run(&["lsusb"]).await;
    list.extend(devices::parse_lsusb(&usb.stdout));
    for (dir, name) in sys_entries("/sys/block") {
        if name.starts_with("loop") || name.starts_with("zram") || name.starts_with("ram") || name.starts_with("dm-") {
            continue;
        }
        if let Some(model) = read(&dir, "device/model").filter(|m| !m.is_empty()) {
            list.push(Device { category: Category::Disks, name: model, driver: None });
        }
    }
    list
}

fn sys_entries(path: &str) -> Vec<(std::path::PathBuf, String)> {
    let Ok(rd) = std::fs::read_dir(path) else { return Vec::new() };
    let mut v: Vec<_> = rd.flatten().map(|e| (e.path(), e.file_name().to_string_lossy().to_string())).collect();
    v.sort_by(|a, b| a.1.cmp(&b.1));
    v
}

fn read(dir: &std::path::Path, file: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(file)).ok().map(|s| s.trim().to_string())
}
