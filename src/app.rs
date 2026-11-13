use std::sync::mpsc::Receiver;
use std::time::Duration;

use egui::{Align, Color32, Layout, RichText};

use crate::backends;
use crate::exec::Report;
use crate::model::{Confidence, Leftover, Package, Source};
use crate::scan::{self, DeleteReport};
use crate::util::fmt_size;

const BG: Color32 = Color32::from_rgb(21, 23, 28);
const PANEL: Color32 = Color32::from_rgb(27, 30, 36);
const CARD: Color32 = Color32::from_rgb(33, 37, 45);
const ACCENT: Color32 = Color32::from_rgb(0x4f, 0xa3, 0xff);
const DANGER: Color32 = Color32::from_rgb(0xd9, 0x45, 0x45);
const WARN: Color32 = Color32::from_rgb(0xff, 0x9d, 0x3c);
const MUTED: Color32 = Color32::from_rgb(0x93, 0x9a, 0xa6);

enum Msg {
    Packages {
        packages: Vec<Package>,
        errors: Vec<String>,
    },
    Uninstalled {
        pkg: Package,
        report: Result<Report, String>,
        scan_after: bool,
    },
    Scanned {
        pkg: Package,
        items: Vec<Leftover>,
    },
    Deleted {
        report: DeleteReport,
        freed: u64,
    },
}

struct ConfirmUninstall {
    pkg: Package,
    scan_after: bool,
    delete_data: bool,
}

struct Review {
    pkg: Package,
    items: Vec<Leftover>,
    checked: Vec<bool>,
}

enum ReviewAction {
    None,
    Close,
    AskDelete,
}

/// Filter between GUI apps (with a .desktop entry) and other packages.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    All,
    Apps,
    Packages,
}

pub struct RevoApp {
    packages: Vec<Package>,
    selected: Option<String>,
    search: String,
    filter: Option<Source>,
    kind: Kind,
    hovered: Option<String>,
    busy: Option<String>,
    rx: Option<Receiver<Msg>>,
    confirm_uninstall: Option<ConfirmUninstall>,
    review: Option<Review>,
    confirm_delete: bool,
    log: Vec<String>,
    status: String,
}

impl RevoApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let mut app = RevoApp {
            packages: Vec::new(),
            selected: None,
            search: std::env::var("REVOLINUX_SEARCH").unwrap_or_default(),
            filter: None,
            kind: Kind::All,
            hovered: None,
            busy: None,
            rx: None,
            confirm_uninstall: None,
            review: None,
            confirm_delete: false,
            log: Vec::new(),
            status: String::new(),
        };
        app.log(format!(
            "RevoLinux {} - clean uninstaller for Fedora (dnf + flatpak)",
            env!("CARGO_PKG_VERSION")
        ));
        app.start_load();
        app
    }

    // ---------- background tasks ----------

    fn spawn_task<F>(&mut self, label: String, task: F)
    where
        F: FnOnce(std::sync::mpsc::Sender<Msg>) + Send + 'static,
    {
        let (tx, rx) = std::sync::mpsc::channel();
        self.rx = Some(rx);
        self.busy = Some(label);
        std::thread::spawn(move || task(tx));
    }

    fn log(&mut self, line: impl Into<String>) {
        self.log.push(line.into());
        if self.log.len() > 600 {
            self.log.drain(0..100);
        }
    }

    fn start_load(&mut self) {
        self.start_scan_none();
        self.spawn_task("Loading installed packages...".to_string(), |tx| {
            let (packages, errors) = backends::list_all();
            let _ = tx.send(Msg::Packages { packages, errors });
        });
    }

    fn start_scan_none(&mut self) {
        self.review = None;
    }

    fn start_uninstall(&mut self, c: ConfirmUninstall) {
        let pkg = c.pkg.clone();
        let label = format!("Uninstalling {}...", pkg.name);
        self.log(format!(
            "Uninstalling {} ({}, {})",
            pkg.name,
            pkg.source.label(),
            pkg.id
        ));
        let scan_after = c.scan_after;
        let delete_data = c.delete_data;
        self.spawn_task(label, move |tx| {
            let report = backends::uninstall(&pkg, delete_data).map_err(|e| e.to_string());
            let _ = tx.send(Msg::Uninstalled {
                pkg,
                report,
                scan_after,
            });
        });
    }

    fn start_scan(&mut self, pkg: Package) {
        let label = format!("Scanning leftovers for {}...", pkg.name);
        self.spawn_task(label, move |tx| {
            let items = scan::scan_package(&pkg);
            let _ = tx.send(Msg::Scanned { pkg, items });
        });
    }

    fn selected_leftovers(&self) -> (Vec<Leftover>, u64) {
        let Some(review) = &self.review else {
            return (Vec::new(), 0);
        };
        let mut items = Vec::new();
        let mut size = 0;
        for (item, checked) in review.items.iter().zip(review.checked.iter()) {
            if *checked {
                items.push(item.clone());
                size += item.size;
            }
        }
        (items, size)
    }

    fn start_delete(&mut self) {
        let (items, freed) = self.selected_leftovers();
        if items.is_empty() {
            return;
        }
        self.log(format!(
            "Deleting {} leftover paths ({})...",
            items.len(),
            fmt_size(freed)
        ));
        self.spawn_task(
            format!("Deleting {} leftover paths...", items.len()),
            move |tx| {
                let report = scan::delete_items(&items);
                let _ = tx.send(Msg::Deleted { report, freed });
            },
        );
    }

    fn poll(&mut self) {
        let msgs: Vec<Msg> = match &self.rx {
            Some(rx) => std::iter::from_fn(|| rx.try_recv().ok()).collect(),
            None => Vec::new(),
        };
        for msg in msgs {
            match msg {
                Msg::Packages { packages, errors } => {
                    self.busy = None;
                    self.rx = None;
                    let count = packages.len();
                    self.packages = packages;
                    if self.selected.is_none() {
                        self.selected = self.packages.first().map(|p| p.key());
                    }
                    let total: u64 = self.packages.iter().map(|p| p.size).sum();
                    self.status = format!("{count} applications, {} installed", fmt_size(total));
                    self.log(self.status.clone());
                    for e in errors {
                        self.log(format!("Backend error: {e}"));
                    }
                    // Developer aid: preselect an app, or open its leftover
                    // review, by ID.
                    if let Ok(id) = std::env::var("REVOLINUX_START_SCAN") {
                        if let Some(pkg) = self
                            .packages
                            .iter()
                            .find(|p| p.id == id || p.name == id)
                            .cloned()
                        {
                            self.selected = Some(pkg.key());
                            self.start_scan(pkg);
                        }
                    } else if let Ok(id) = std::env::var("REVOLINUX_SELECT") {
                        if let Some(pkg) = self.packages.iter().find(|p| p.id == id) {
                            self.selected = Some(pkg.key());
                        }
                    }
                }
                Msg::Uninstalled {
                    pkg,
                    report,
                    scan_after,
                } => {
                    self.busy = None;
                    self.rx = None;
                    match report {
                        Ok(r) if r.ok => {
                            if !r.text().is_empty() {
                                self.log(r.text());
                            }
                            self.log(format!("Uninstalled {}", pkg.id));
                            self.packages.retain(|p| p.key() != pkg.key());
                            if self.selected.as_deref() == Some(pkg.key().as_str()) {
                                self.selected = self.packages.first().map(|p| p.key());
                            }
                            if scan_after {
                                self.start_scan(pkg);
                            } else {
                                self.status = format!("Uninstalled {}", pkg.name);
                                self.review = None;
                            }
                        }
                        Ok(r) => {
                            self.status = format!("Uninstall failed for {}", pkg.name);
                            self.log(format!("Uninstall failed (exit {:?})", r.code));
                            self.log(r.text());
                        }
                        Err(e) => {
                            self.status = format!("Uninstall failed for {}", pkg.name);
                            self.log(format!("Uninstall failed: {e}"));
                        }
                    }
                }
                Msg::Scanned { pkg, items } => {
                    self.busy = None;
                    self.rx = None;
                    let total: u64 = items.iter().map(|i| i.size).sum();
                    let checked = items
                        .iter()
                        .map(|i| i.confidence >= Confidence::Medium)
                        .collect();
                    self.status = format!(
                        "{} leftover items for {} ({})",
                        items.len(),
                        pkg.name,
                        fmt_size(total)
                    );
                    self.log(self.status.clone());
                    self.log(format!(
                        "Leftover scan for {}: {} items, {}",
                        pkg.name,
                        items.len(),
                        fmt_size(total)
                    ));
                    self.review = Some(Review {
                        pkg,
                        items,
                        checked,
                    });
                }
                Msg::Deleted { report, freed } => {
                    self.busy = None;
                    self.rx = None;
                    if !report.root_output.is_empty() {
                        self.log(report.root_output.clone());
                    }
                    for f in &report.failures {
                        self.log(format!("Delete failed: {f}"));
                    }
                    self.status = format!(
                        "Deleted {} paths, {} freed",
                        report.deleted,
                        fmt_size(freed)
                    );
                    self.log(self.status.clone());
                    if report.failures.is_empty() {
                        self.review = None;
                    } else if let Some(mut review) = self.review.take() {
                        review.items.retain(|i| i.path.exists());
                        review.checked = vec![true; review.items.len()];
                        self.review = Some(review);
                    }
                }
            }
        }
    }

    // ---------- UI ----------

    fn filtered(&self) -> Vec<usize> {
        let needle = self.search.trim();
        let mut hits: Vec<(usize, i32)> = self
            .packages
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let source_ok = self.filter.map(|f| f == p.source).unwrap_or(true);
                let kind_ok = match self.kind {
                    Kind::All => true,
                    Kind::Apps => p.gui,
                    Kind::Packages => !p.gui,
                };
                if !source_ok || !kind_ok {
                    return None;
                }
                if needle.is_empty() {
                    return Some((i, 0));
                }
                crate::search::score(needle, &p.name, &p.id, &p.summary).map(|s| (i, s))
            })
            .collect();

        if !needle.is_empty() {
            hits.sort_by(|a, b| {
                b.1.cmp(&a.1).then_with(|| {
                    let a_name = self.packages[a.0].name.to_lowercase();
                    let b_name = self.packages[b.0].name.to_lowercase();
                    a_name
                        .len()
                        .cmp(&b_name.len())
                        .then_with(|| a_name.cmp(&b_name))
                })
            });
        }
        hits.into_iter().map(|(i, _)| i).collect()
    }

    fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(16.0, 10.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("RevoLinux").size(22.0).strong().color(ACCENT));
                    ui.label(
                        RichText::new("clean uninstaller for Fedora")
                            .size(12.0)
                            .color(MUTED),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let refresh =
                            ui.add_enabled(self.busy.is_none(), egui::Button::new("Refresh"));
                        if refresh.clicked() {
                            self.start_load();
                        }
                        let count = self.packages.len();
                        let total: u64 = self.packages.iter().map(|p| p.size).sum();
                        ui.label(
                            RichText::new(format!("{count} installed · {}", fmt_size(total)))
                                .color(MUTED)
                                .size(12.0),
                        );
                    });
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("Search: vsc, vscode, fox...")
                            .desired_width(280.0),
                    );
                    ui.add_space(6.0);
                    ui.selectable_value(&mut self.filter, None, "All");
                    ui.selectable_value(&mut self.filter, Some(Source::Dnf), "DNF");
                    ui.selectable_value(&mut self.filter, Some(Source::Flatpak), "Flatpak");
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(RichText::new("Show").color(MUTED).size(12.0));
                    ui.selectable_value(&mut self.kind, Kind::All, "All");
                    ui.selectable_value(&mut self.kind, Kind::Apps, "Applications")
                        .on_hover_text("Packages with a .desktop entry, shown in the app menu");
                    ui.selectable_value(&mut self.kind, Kind::Packages, "Packages")
                        .on_hover_text("Libraries, system packages and other non-GUI packages");
                });
            });
    }

    fn package_list(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("packages")
            .resizable(true)
            .default_width(390.0)
            .min_width(280.0)
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(10.0, 8.0)),
            )
            .show(ctx, |ui| {
                let indices = self.filtered();
                // Keep the details pane in sync with the visible results.
                let selection_visible = self
                    .selected
                    .as_ref()
                    .map(|key| indices.iter().any(|i| self.packages[*i].key() == *key))
                    .unwrap_or(false);
                if !selection_visible {
                    self.selected = indices.first().map(|i| self.packages[*i].key());
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Installed").strong());
                    ui.label(RichText::new(format!("({})", indices.len())).color(MUTED));
                });
                ui.add_space(4.0);
                if self.packages.is_empty() && self.busy.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Loading...");
                    });
                    return;
                }
                if indices.is_empty() {
                    ui.label(RichText::new("No matching applications.").color(MUTED));
                    let query = self.search.trim().to_string();
                    if !query.is_empty() {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Already uninstalled? Scan its leftover files by name.")
                                .color(MUTED)
                                .size(11.0),
                        );
                        if ui
                            .button(format!("Scan leftovers for \"{query}\""))
                            .clicked()
                        {
                            self.start_scan(Package::synthetic(&query));
                        }
                    }
                    return;
                }
                self.hovered = None;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for idx in indices {
                            let p = &self.packages[idx];
                            let key = p.key();
                            let selected = self.selected.as_deref() == Some(key.as_str());
                            let hovered = self.hovered.as_deref() == Some(key.as_str());
                            let bg = if selected {
                                egui::Color32::from_rgb(0x2c, 0x5f, 0x9e)
                            } else if hovered {
                                CARD
                            } else {
                                Color32::TRANSPARENT
                            };
                            let frame = egui::Frame::none()
                                .fill(bg)
                                .rounding(egui::Rounding::same(6.0))
                                .inner_margin(egui::Margin::symmetric(6.0, 4.0))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        match p.icon.as_ref() {
                                            Some(path) => {
                                                let uri = format!("file://{}", path.display());
                                                ui.add(
                                                    egui::Image::new(egui::ImageSource::Uri(
                                                        uri.into(),
                                                    ))
                                                    .fit_to_exact_size(egui::vec2(32.0, 32.0)),
                                                );
                                            }
                                            None => {
                                                let (rect, _) = ui.allocate_exact_size(
                                                    egui::vec2(32.0, 32.0),
                                                    egui::Sense::hover(),
                                                );
                                                ui.painter().circle_filled(
                                                    rect.center(),
                                                    15.0,
                                                    CARD,
                                                );
                                                let initial = p
                                                    .name
                                                    .chars()
                                                    .next()
                                                    .unwrap_or('?')
                                                    .to_uppercase()
                                                    .to_string();
                                                ui.painter().text(
                                                    rect.center(),
                                                    egui::Align2::CENTER_CENTER,
                                                    initial,
                                                    egui::FontId::proportional(16.0),
                                                    p.source.color(),
                                                );
                                            }
                                        }
                                        ui.vertical(|ui| {
                                            ui.add(
                                                egui::Label::new(RichText::new(&p.name).size(13.0))
                                                    .truncate(),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(format!(
                                                        "{} · {} · {}",
                                                        p.source.label(),
                                                        p.version,
                                                        fmt_size(p.size)
                                                    ))
                                                    .size(11.0)
                                                    .color(MUTED),
                                                )
                                                .truncate(),
                                            );
                                        });
                                    });
                                });
                            let resp = ui
                                .interact(
                                    frame.response.rect,
                                    egui::Id::new(("pkg", key.as_str())),
                                    egui::Sense::click(),
                                )
                                .on_hover_text(p.id.clone());
                            if resp.hovered() {
                                self.hovered = Some(key.clone());
                            }
                            if resp.clicked() {
                                self.selected = Some(key);
                                self.review = None;
                            }
                            ui.add_space(2.0);
                        }
                    });
            });
    }

    fn details(&mut self, ui: &mut egui::Ui) {
        let Some(key) = self.selected.clone() else {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("Select an application to uninstall.")
                        .size(16.0)
                        .color(MUTED),
                );
            });
            return;
        };
        let Some(pkg) = self.packages.iter().find(|p| p.key() == key).cloned() else {
            ui.label(RichText::new("Application no longer installed.").color(MUTED));
            return;
        };

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if let Some(path) = pkg.icon.as_ref() {
                let uri = format!("file://{}", path.display());
                ui.add(
                    egui::Image::new(egui::ImageSource::Uri(uri.into()))
                        .fit_to_exact_size(egui::vec2(44.0, 44.0)),
                );
            }
            ui.heading(RichText::new(&pkg.name).size(24.0));
            ui.add_space(6.0);
            ui.label(
                RichText::new(pkg.source.label())
                    .color(pkg.source.color())
                    .strong()
                    .size(11.0),
            );
            if pkg.source == Source::Flatpak && !pkg.installation.is_empty() {
                ui.label(
                    RichText::new(format!("({})", pkg.installation))
                        .color(MUTED)
                        .size(11.0),
                );
            }
        });
        ui.label(RichText::new(&pkg.id).monospace().color(MUTED).size(12.0));
        ui.add_space(10.0);

        egui::Frame::none()
            .fill(CARD)
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::Grid::new("pkg-meta")
                    .num_columns(2)
                    .spacing([18.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("Version").color(MUTED));
                        ui.label(&pkg.version);
                        ui.end_row();
                        ui.label(RichText::new("Installed size").color(MUTED));
                        ui.label(fmt_size(pkg.size));
                        ui.end_row();
                        ui.label(RichText::new("Backend").color(MUTED));
                        ui.label(format!(
                            "{} ({})",
                            pkg.source.label(),
                            match pkg.source {
                                Source::Dnf => "rpm package",
                                Source::Flatpak => "sandboxed app",
                            }
                        ));
                        ui.end_row();
                        ui.label(RichText::new("Menu entry").color(MUTED));
                        ui.label(if pkg.gui { "yes" } else { "no desktop entry" });
                        ui.end_row();
                    });
                if !pkg.summary.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(&pkg.summary).color(MUTED).size(12.0));
                }
            });

        ui.add_space(14.0);
        ui.label(
            RichText::new(
                "Uninstall removes the package first. RevoLinux then scans config, cache, \
                 data, temp and desktop files belonging to the application and asks before \
                 deleting each leftover path.",
            )
            .color(MUTED)
            .size(12.0),
        );
        ui.add_space(14.0);

        ui.horizontal(|ui| {
            let btn = egui::Button::new(
                RichText::new("Uninstall / Purge...")
                    .strong()
                    .color(Color32::WHITE),
            )
            .fill(DANGER)
            .min_size(egui::vec2(170.0, 34.0));
            if ui.add_enabled(self.busy.is_none(), btn).clicked() {
                self.confirm_uninstall = Some(ConfirmUninstall {
                    pkg: pkg.clone(),
                    scan_after: true,
                    delete_data: false,
                });
            }
        });
    }

    fn review_panel(&mut self, ui: &mut egui::Ui) {
        let action = {
            let Some(review) = self.review.as_mut() else {
                return;
            };
            review_ui(ui, review)
        };
        match action {
            ReviewAction::Close => self.review = None,
            ReviewAction::AskDelete => self.confirm_delete = true,
            ReviewAction::None => {}
        }
    }

    fn modals(&mut self, ctx: &egui::Context) {
        if let Some(c) = &mut self.confirm_uninstall {
            let mut cancel = false;
            let mut go = false;
            egui::Window::new("Uninstall application")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.set_min_width(430.0);
                    ui.label(RichText::new(&c.pkg.name).size(18.0).strong());
                    ui.label(RichText::new(&c.pkg.id).monospace().color(MUTED).size(11.0));
                    ui.add_space(8.0);
                    ui.label(
                        "This removes the installed package with its package manager.",
                    );
                    ui.add_space(8.0);
                    ui.checkbox(
                        &mut c.scan_after,
                        "Scan for leftover files after removal",
                    );
                    if c.pkg.source == Source::Flatpak {
                        ui.checkbox(
                            &mut c.delete_data,
                            "Also delete flatpak app data (~/.var/app) now",
                        );
                        ui.label(
                            RichText::new(
                                "Leave unchecked to review the app data folder in the leftover list.",
                            )
                            .color(MUTED)
                            .size(11.0),
                        );
                    }
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "DNF may also remove unneeded dependencies. Deletion is permanent.",
                        )
                        .color(WARN)
                        .size(11.0),
                    );
                    ui.label(
                        RichText::new(match c.pkg.source {
                            Source::Dnf => {
                                "Admin authentication (polkit) is required for system packages."
                            }
                            Source::Flatpak if c.pkg.installation == "user" => {
                                "User flatpak: no administrator password needed."
                            }
                            Source::Flatpak => {
                                "System flatpak: authentication may be required."
                            }
                        })
                        .color(MUTED)
                        .size(11.0),
                    );
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                        let btn = egui::Button::new(
                            RichText::new("Uninstall").strong().color(Color32::WHITE),
                        )
                        .fill(DANGER);
                        if ui.add(btn).clicked() {
                            go = true;
                        }
                    });
                });
            if cancel {
                self.confirm_uninstall = None;
            } else if go {
                if let Some(c) = self.confirm_uninstall.take() {
                    self.start_uninstall(c);
                }
            }
        }

        if self.confirm_delete {
            let (items, size) = self.selected_leftovers();
            let needs_root = items.iter().any(|i| i.needs_root);
            let mut cancel = false;
            let mut go = false;
            egui::Window::new("Delete leftover files")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.set_min_width(460.0);
                    ui.label(
                        RichText::new(format!(
                            "Permanently delete {} paths?",
                            items.len()
                        ))
                        .size(17.0)
                        .strong(),
                    );
                    ui.label(RichText::new(format!("{} will be freed.", fmt_size(size))).color(MUTED));
                    ui.add_space(8.0);
                    for item in items.iter().take(8) {
                        ui.label(
                            RichText::new(item.display_path())
                                .monospace()
                                .size(11.0)
                                .color(MUTED),
                        );
                    }
                    if items.len() > 8 {
                        ui.label(
                            RichText::new(format!("... and {} more", items.len() - 8))
                                .color(MUTED)
                                .size(11.0),
                        );
                    }
                    if needs_root {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(
                                "Some paths are system files. A polkit password prompt will appear.",
                            )
                            .color(WARN)
                            .size(11.0),
                        );
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                        let btn = egui::Button::new(
                            RichText::new("Delete permanently")
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(DANGER);
                        if ui.add(btn).clicked() {
                            go = true;
                        }
                    });
                });
            if cancel {
                self.confirm_delete = false;
            } else if go {
                self.confirm_delete = false;
                self.start_delete();
            }
        }
    }
}

fn review_ui(ui: &mut egui::Ui, review: &mut Review) -> ReviewAction {
    let mut action = ReviewAction::None;
    let selected_count = review.checked.iter().filter(|c| **c).count();
    let selected_size: u64 = review
        .items
        .iter()
        .zip(review.checked.iter())
        .filter(|(_, c)| **c)
        .map(|(i, _)| i.size)
        .sum();
    let total: u64 = review.items.iter().map(|i| i.size).sum();

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.heading(RichText::new("Leftover scan").size(22.0));
        ui.label(
            RichText::new(&review.pkg.name)
                .color(review.pkg.source.color())
                .strong(),
        );
        ui.label(
            RichText::new(format!(
                "{} items · {}",
                review.items.len(),
                fmt_size(total)
            ))
            .color(MUTED)
            .size(12.0),
        );
    });
    ui.label(
        RichText::new(
            "Choose what to delete. HIGH/MED items are preselected; LOW items are unchecked.",
        )
        .color(MUTED)
        .size(12.0),
    );
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("Select all").clicked() {
            review.checked.iter_mut().for_each(|c| *c = true);
        }
        if ui.button("Select none").clicked() {
            review.checked.iter_mut().for_each(|c| *c = false);
        }
        if ui.button("Select high only").clicked() {
            for (item, checked) in review.items.iter().zip(review.checked.iter_mut()) {
                *checked = item.confidence == Confidence::High;
            }
        }
        ui.add_space(12.0);
        let delete_label = format!(
            "Delete selected ({} · {})",
            selected_count,
            fmt_size(selected_size)
        );
        let btn = egui::Button::new(RichText::new(delete_label).color(Color32::WHITE).strong())
            .fill(if selected_count > 0 { DANGER } else { CARD });
        if ui.add_enabled(selected_count > 0, btn).clicked() {
            action = ReviewAction::AskDelete;
        }
        if ui.button("Close").clicked() {
            action = ReviewAction::Close;
        }
    });
    ui.add_space(6.0);
    ui.separator();

    if review.items.is_empty() {
        ui.add_space(20.0);
        ui.label(
            RichText::new("No leftover files found for this application.")
                .size(15.0)
                .color(MUTED),
        );
        return action;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let Review {
                items,
                checked,
                pkg: _,
            } = review;
            for (item, checked) in items.iter().zip(checked.iter_mut()) {
                let row = ui.horizontal(|ui| {
                    ui.checkbox(checked, "");
                    ui.label(
                        RichText::new(item.confidence.label())
                            .color(item.confidence.color())
                            .size(10.0)
                            .strong(),
                    );
                    if item.needs_root {
                        ui.label(RichText::new("ROOT").color(WARN).size(10.0).strong());
                    }
                    let path_label =
                        egui::Label::new(RichText::new(item.display_path()).monospace().size(12.0))
                            .truncate();
                    ui.add(path_label);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(fmt_size(item.size)).color(MUTED).size(11.0));
                    });
                });
                row.response.on_hover_text(format!(
                    "{}\nroot: {}\nmatched token: {}\n{} files",
                    item.path.display(),
                    item.root,
                    item.token,
                    item.files
                ));
                ui.add_space(2.0);
            }
        });
    action
}

fn configure_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = Color32::from_rgb(16, 18, 22);
    visuals.faint_bg_color = CARD;
    visuals.selection.bg_fill = Color32::from_rgb(0x2c, 0x5f, 0x9e);
    visuals.selection.stroke.color = Color32::WHITE;
    visuals.hyperlink_color = ACCENT;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(45, 50, 60);
    visuals.widgets.active.bg_fill = Color32::from_rgb(52, 58, 70);
    visuals.window_rounding = egui::Rounding::same(10.0);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.spacing.interact_size.y = 26.0;
    ctx.set_style(style);
}

impl eframe::App for RevoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();

        self.header(ctx);
        egui::TopBottomPanel::bottom("log")
            .resizable(true)
            .default_height(140.0)
            .min_height(80.0)
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(14.0, 8.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Log").strong());
                    if let Some(busy) = &self.busy {
                        ui.spinner();
                        ui.label(RichText::new(busy).color(ACCENT).size(12.0));
                    } else if !self.status.is_empty() {
                        ui.label(RichText::new(&self.status).color(MUTED).size(12.0));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Clear").clicked() {
                            self.log.clear();
                        }
                    });
                });
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for line in &self.log {
                            ui.label(
                                RichText::new(line)
                                    .monospace()
                                    .size(11.0)
                                    .color(Color32::from_rgb(0xb8, 0xbe, 0xc8)),
                            );
                        }
                    });
            });

        self.package_list(ctx);

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(20.0, 10.0)),
            )
            .show(ctx, |ui| {
                if self.review.is_some() {
                    self.review_panel(ui);
                } else {
                    self.details(ui);
                }
            });

        self.modals(ctx);

        if self.busy.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}
