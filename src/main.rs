#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod collections;
mod curl;
mod model;
mod send;
mod storage;
mod theme;

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use egui::{Context, RichText};
use reqwest::cookie::Jar;

use collections::Collection;
use model::{pretty_body, substitute, BodyMode, Kv, RequestDef, ResponseBundle, METHODS};
use storage::{Environment, HistoryRecord, Workspace};
use theme::{apply, Palette, Theme};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReqView {
    Params,
    Headers,
    Body,
    Auth,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RespView {
    Pretty,
    Raw,
    Headers,
    Cookies,
    Code,
    Meta,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sidebar {
    Collections,
    Environments,
    History,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum IoMode {
    Import,
    Export,
}

const APP_NAME: &str = "Techvision API Checker";

const CODE_LANGS: &[&str] = &[
    "cURL (bash)",
    "cURL (Windows)",
    "PowerShell",
    "Python requests",
    "JavaScript fetch",
];

#[derive(Clone, PartialEq)]
enum Pending {
    DeleteCollection(usize),
    DeleteEnvironment(usize),
    DeleteRequest {
        collection: usize,
        folder: Option<String>,
        index: usize,
    },
    ClearHistory,
    ReloadWorkspace,
}

enum Dialog {
    Save {
        name: String,
        collection: usize,
        folder: String,
    },
    Io {
        mode: IoMode,
        path: String,
        content: String,
        collection: usize,
    },
    Confirm {
        title: String,
        message: String,
        pending: Pending,
    },
    Settings,
    About,
}

struct Tab {
    id: u64,
    req: RequestDef,
    resp: Option<ResponseBundle>,
    sending: bool,
    req_view: ReqView,
    resp_view: RespView,
    code_lang: usize,
}

pub struct App {
    ws: Workspace,
    tabs: Vec<Tab>,
    active: usize,
    next_id: u64,
    tx: Sender<(u64, ResponseBundle)>,
    rx: Receiver<(u64, ResponseBundle)>,
    rt: tokio::runtime::Runtime,
    jar: Arc<Jar>,
    sidebar: Sidebar,
    sidebar_visible: bool,
    history_filter: String,
    save_body_path: String,
    dialog: Option<Dialog>,
    toast: Option<(String, Instant)>,
    toast_error: bool,
    last_autosave: Instant,
    dirty: bool,
    theme_applied: Option<(Theme, Option<egui::Theme>)>,
    palette: Palette,
}

fn blank_request() -> RequestDef {
    RequestDef {
        name: "New request".to_string(),
        method: "GET".to_string(),
        headers: vec![Kv::new("Accept", "application/json")],
        ..Default::default()
    }
}

impl Default for App {
    fn default() -> Self {
        Self::with_workspace(storage::load())
    }
}

impl App {
    fn with_workspace(ws: Workspace) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            ws,
            tabs: vec![Tab {
                id: 0,
                req: blank_request(),
                resp: None,
                sending: false,
                req_view: ReqView::Headers,
                resp_view: RespView::Pretty,
                code_lang: 0,
            }],
            active: 0,
            next_id: 1,
            tx,
            rx,
            rt: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to start the async runtime"),
            jar: Arc::new(Jar::default()),
            sidebar: Sidebar::Collections,
            sidebar_visible: true,
            history_filter: String::new(),
            save_body_path: storage::config_path()
                .join("response.json")
                .display()
                .to_string(),
            dialog: None,
            toast: None,
            toast_error: false,
            last_autosave: Instant::now(),
            dirty: false,
            theme_applied: None,
            palette: Palette::dark(),
        }
    }
}


fn human_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn auth_kind(auth: &model::Auth) -> usize {
    match auth {
        model::Auth::NoAuth => 0,
        model::Auth::Bearer { .. } => 1,
        model::Auth::Basic { .. } => 2,
        model::Auth::ApiKey { .. } => 3,
    }
}

fn set_auth_kind(req: &mut RequestDef, kind: usize) {
    req.auth = match kind {
        1 => model::Auth::Bearer {
            token: String::new(),
        },
        2 => model::Auth::Basic {
            username: String::new(),
            password: String::new(),
        },
        3 => model::Auth::ApiKey {
            key: String::new(),
            value: String::new(),
            location: model::ApiKeyIn::Header,
        },
        _ => model::Auth::NoAuth,
    };
}

/// Splits the URL's own query string into the params grid so the two editors
/// never fight over the same data.
fn pull_query_into_params(url: &mut String, params: &mut Vec<Kv>) {
    if let Some((base, query)) = url.split_once('?') {
        for seg in query.split('&').filter(|s| !s.is_empty()) {
            let (k, v) = seg.split_once('=').unwrap_or((seg, ""));
            if !params.iter().any(|p| p.key == k) {
                params.push(Kv::new(k, v));
            }
        }
        *url = base.to_string();
    }
}

fn kv_grid(ui: &mut egui::Ui, id: &str, rows: &mut Vec<Kv>) {
    let mut remove: Option<usize> = None;
    egui::Grid::new(id)
        .num_columns(5)
        .spacing([6.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            ui.strong("");
            ui.strong("Key");
            ui.strong("Value");
            ui.strong("Description");
            ui.end_row();
            for (i, row) in rows.iter_mut().enumerate() {
                ui.checkbox(&mut row.enabled, "");
                ui.add(egui::TextEdit::singleline(&mut row.key).hint_text("Key"));
                ui.add(
                    egui::TextEdit::singleline(&mut row.value)
                        .hint_text("Value")
                        .desired_width(260.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut row.description)
                        .hint_text("optional")
                        .desired_width(140.0),
                );
                if ui.small_button("✕").on_hover_text("Remove row").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    if let Some(i) = remove {
        rows.remove(i);
    }
}

/// Compact editable rows for the sidebar. The key is a real text field, so a
/// freshly added variable can be named; returns the index the user deleted.
fn kv_editor(ui: &mut egui::Ui, id: &str, rows: &mut Vec<Kv>, palette: Palette) -> Option<usize> {
    let mut remove: Option<usize> = None;
    ui.push_id(id, |ui| {
        for (i, row) in rows.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.checkbox(&mut row.enabled, "")
                    .on_hover_text("Include this variable");
                ui.add(
                    egui::TextEdit::singleline(&mut row.key)
                        .hint_text("name")
                        .font(egui::TextStyle::Monospace)
                        .desired_width(96.0),
                );
                ui.label("=");
                ui.add(
                    egui::TextEdit::singleline(&mut row.value)
                        .hint_text("value")
                        .desired_width(130.0),
                );
                if ui
                    .small_button("✕")
                    .on_hover_text("Delete variable")
                    .clicked()
                {
                    remove = Some(i);
                }
            });
            if row.key.trim().is_empty() {
                ui.indent(format!("blank-{i}"), |ui| {
                    ui.label(
                        RichText::new("give this variable a name or delete the row")
                            .small()
                            .color(palette.warn),
                    );
                });
            }
        }
    });
    remove
}

impl App {
    fn notify(&mut self, msg: impl Into<String>, error: bool) {
        self.toast_error = error;
        self.toast = Some((msg.into(), Instant::now()));
    }

    fn open_tab(&mut self, req: RequestDef) {
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(Tab {
            id,
            req,
            resp: None,
            sending: false,
            req_view: ReqView::Headers,
            resp_view: RespView::Pretty,
            code_lang: 0,
        });
        self.active = self.tabs.len() - 1;
    }

    fn close_tab(&mut self, idx: usize) {
        if idx >= self.tabs.len() {
            return;
        }
        if self.tabs.len() == 1 {
            self.notify("Close the last tab? Use File ▸ Quit to exit.", false);
            return;
        }
        self.tabs.remove(idx);
        self.active = self.active.min(self.tabs.len() - 1);
    }

    fn send_active(&mut self) {
        let idx = self.active;
        let Some(tab) = self.tabs.get(idx) else { return };
        if tab.sending {
            return;
        }
        let req = tab.req.clone();
        let id = tab.id;
        if req.url.trim().is_empty() {
            self.notify("Enter a URL before sending.", true);
            return;
        }
        let vars = self.ws.vars();
        let mut missing: Vec<String> = Vec::new();
        for text in [&req.url, &req.body] {
            for name in self.ws.unresolved(text) {
                if !missing.contains(&name) {
                    missing.push(name);
                }
            }
        }
        if !missing.is_empty() {
            self.notify(
                format!("Unresolved {{var}} sent literally: {}", missing.join(", ")),
                true,
            );
        }
        let jar = self.jar.clone();
        let proxy = self.ws.settings.proxy_url.clone();
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
            tab.sending = true;
            tab.resp = None;
        }
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let proxy: Option<&str> = if proxy.trim().is_empty() {
                None
            } else {
                Some(proxy.trim())
            };
            let resp = send::perform(&req, &vars, jar, proxy).await;
            let _ = tx.send((id, resp));
        });
    }

    fn drain_responses(&mut self) {
        while let Ok((id, resp)) = self.rx.try_recv() {
            let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) else {
                continue;
            };
            tab.sending = false;
            let record = HistoryRecord {
                at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                name: tab.req.title(),
                method: tab.req.method.clone(),
                url: tab.req.url.clone(),
                status: if resp.error.is_some() { None } else { Some(resp.status) },
                elapsed_ms: resp.elapsed_ms,
                body_bytes: resp.body_bytes,
                request: tab.req.clone(),
            };
            tab.resp = Some(resp);
            self.ws.history.insert(0, record);
            self.ws.history.truncate(200);
            self.dirty = true;
        }
    }

    fn autosave(&mut self, force: bool) {
        if (!self.dirty && !force) || (!force && self.last_autosave.elapsed() < Duration::from_secs(20))
        {
            return;
        }
        match storage::save(&self.ws) {
            Ok(_) => {
                self.dirty = false;
                self.last_autosave = Instant::now();
            }
            Err(e) => self.notify(format!("Autosave failed: {e}"), true),
        }
    }

    fn save_request_to(&mut self, collection: usize, folder: &str, name: &str) {
        let name = name.trim().to_string();
        if name.is_empty() {
            self.notify("Give the request a name.", true);
            return;
        }
        let Some(col) = self.ws.collections.get_mut(collection) else {
            self.notify("Pick a collection first.", true);
            return;
        };
        let Some(tab) = self.tabs.get_mut(self.active) else { return };
        let mut req = tab.req.clone();
        req.name = name.clone();
        let existing = if folder == "<root>" {
            col.requests.iter_mut().find(|r| r.name == req.name)
        } else {
            col.folders
                .iter_mut()
                .find(|f| f.name == folder)
                .and_then(|f| f.requests.iter_mut().find(|r| r.name == req.name))
        };
        if let Some(slot) = existing {
            *slot = req;
        } else {
            col.add_request(folder, req);
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.req.name = name.clone();
        }
        self.dirty = true;
        self.toast_error = false;
        self.toast = Some((format!("Saved into {} ▸ {folder}", col.name), Instant::now()));
    }

    fn do_pending(&mut self, pending: Pending) {
        match pending {
            Pending::DeleteCollection(i) => {
                if i < self.ws.collections.len() {
                    let removed = self.ws.collections.remove(i);
                    self.notify(format!("Deleted collection {}", removed.name), false);
                    self.dirty = true;
                }
            }
            Pending::DeleteEnvironment(i) => {
                if i < self.ws.environments.len() {
                    self.ws.environments.remove(i);
                    self.dirty = true;
                }
            }
            Pending::DeleteRequest {
                collection,
                folder,
                index,
            } => {
                if let Some(col) = self.ws.collections.get_mut(collection) {
                    match folder {
                        None => {
                            if index < col.requests.len() {
                                col.requests.remove(index);
                            }
                        }
                        Some(name) => {
                            if let Some(f) = col.folders.iter_mut().find(|f| f.name == name) {
                                if index < f.requests.len() {
                                    f.requests.remove(index);
                                }
                            }
                        }
                    }
                    self.dirty = true;
                }
            }
            Pending::ClearHistory => {
                self.ws.history.clear();
                self.dirty = true;
                self.notify("History cleared.", false);
            }
            Pending::ReloadWorkspace => {
                self.ws = storage::load();
                self.tabs.clear();
                self.active = 0;
                self.open_tab(blank_request());
                self.notify("Workspace reloaded from disk.", false);
            }
        }
    }

    fn import_from_path(&mut self, path: &str) {
        match std::fs::read_to_string(path) {
            Ok(text) => self.import_from_text(&text, path),
            Err(e) => self.notify(format!("Could not read {path}: {e}"), true),
        }
    }

    fn import_from_text(&mut self, text: &str, label: &str) {
        match serde_json::from_str::<serde_json::Value>(text) {
            Ok(value) => {
                let col = collections::from_postman_json(&value);
                if col.total_requests() == 0 {
                    self.notify(format!("No requests found in {label} — is it a Postman v2.1 export?"), true);
                    return;
                }
                let n = col.total_requests();
                let name = col.name.clone();
                self.ws.collections.push(col);
                self.dirty = true;
                self.notify(format!("Imported {n} request(s) from \"{name}\""), false);
            }
            Err(e) => self.notify(format!("Invalid JSON in {label}: {e}"), true),
        }
    }

    fn export_collection(&mut self, collection: usize, path: &str) {
        let Some(col) = self.ws.collections.get(collection) else {
            self.notify("Pick a collection to export.", true);
            return;
        };
        let json = collections::to_postman_json(col);
        let text = serde_json::to_string_pretty(&json).unwrap_or_default();
        match std::fs::write(path, &text) {
            Ok(_) => self.notify(
                format!("Exported {} request(s) to {}", col.total_requests(), path),
                false,
            ),
            Err(e) => self.notify(format!("Could not write {path}: {e}"), true),
        }
    }
}

impl App {
    fn update_ui(&mut self, ctx: &Context) {
        let system_theme = ctx.system_theme();
        if self.theme_applied != Some((self.ws.settings.theme, system_theme)) {
            self.palette = apply(ctx, self.ws.settings.theme);
            self.theme_applied = Some((self.ws.settings.theme, system_theme));
        }

        self.drain_responses();
        if self.tabs.iter().any(|t| t.sending) {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
        if self.toast.is_some() {
            ctx.request_repaint_after(Duration::from_millis(600));
        }
        self.autosave(false);

        let mods = ctx.input(|i| i.modifiers);
        let send_now = ctx.input(|i| {
            (i.modifiers.command && i.key_pressed(egui::Key::Enter)) || i.key_pressed(egui::Key::F5)
        });
        if send_now {
            self.send_active();
        }
        if mods.command && !mods.shift && ctx.input(|i| i.key_pressed(egui::Key::T)) {
            self.open_tab(blank_request());
        }
        if mods.command && ctx.input(|i| i.key_pressed(egui::Key::W)) {
            self.close_tab(self.active);
        }
        if mods.command && ctx.input(|i| i.key_pressed(egui::Key::S)) {
            if mods.shift {
                self.autosave(true);
                let p = storage::state_file().display().to_string();
                self.notify(format!("Workspace written to {p}"), false);
            } else {
                let name = self
                    .tabs
                    .get(self.active)
                    .map(|t| t.req.name.clone())
                    .unwrap_or_default();
                self.dialog = Some(Dialog::Save {
                    name: if name == "New request" { String::new() } else { name },
                    collection: 0,
                    folder: "<root>".to_string(),
                });
            }
        }

        egui::TopBottomPanel::bottom("status")
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{} collection(s) · {} variable(s) active",
                        self.ws.collections.len(),
                        self.ws.vars().len()
                    ));
                    ui.separator();
                    if let Some((msg, at)) = self.toast.as_ref() {
                        if at.elapsed() < Duration::from_secs(6) {
                            let color = if self.toast_error {
                                self.palette.err
                            } else {
                                self.palette.ok
                            };
                            ui.colored_label(color, msg.clone());
                        } else {
                            self.toast = None;
                        }
                    }
                    if self.dirty {
                        ui.separator();
                        ui.colored_label(
                            self.palette.warn,
                            "unsaved changes (auto-saves every 20s)",
                        );
                    }
                });
            });

        egui::TopBottomPanel::top("menubar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button("File", |ui| {
                    if ui
                        .add(egui::Button::new("New request tab").shortcut_text("Ctrl+T"))
                        .clicked()
                    {
                        self.open_tab(blank_request());
                        ui.close_menu();
                    }
                    if ui
                        .add(egui::Button::new("Save request to collection").shortcut_text("Ctrl+S"))
                        .clicked()
                    {
                        self.dialog = Some(Dialog::Save {
                            name: String::new(),
                            collection: 0,
                            folder: "<root>".to_string(),
                        });
                        ui.close_menu();
                    }
                    if ui.button("Import Postman collection…").clicked() {
                        self.dialog = Some(Dialog::Io {
                            mode: IoMode::Import,
                            path: String::new(),
                            content: String::new(),
                            collection: 0,
                        });
                        ui.close_menu();
                    }
                    if ui.button("Export collection as Postman JSON…").clicked() {
                        let path = storage::config_path()
                            .join("collection.json")
                            .display()
                            .to_string();
                        self.dialog = Some(Dialog::Io {
                            mode: IoMode::Export,
                            path,
                            content: String::new(),
                            collection: 0,
                        });
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui
                        .add(egui::Button::new("Save workspace now").shortcut_text("Ctrl+Shift+S"))
                        .clicked()
                    {
                        self.autosave(true);
                        let p = storage::state_file().display().to_string();
                        self.notify(format!("Workspace written to {p}"), false);
                        ui.close_menu();
                    }
                    if ui.button("Reload workspace from disk").clicked() {
                        self.dialog = Some(Dialog::Confirm {
                            title: "Reload workspace".to_string(),
                            message: "Discard in-memory edits and read workspace.json again?"
                                .to_string(),
                            pending: Pending::ReloadWorkspace,
                        });
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        self.autosave(true);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Duplicate current tab").clicked() {
                        if let Some(tab) = self.tabs.get(self.active) {
                            self.open_tab(tab.req.clone());
                        }
                        ui.close_menu();
                    }
                    if ui.button("Clear current response").clicked() {
                        if let Some(tab) = self.tabs.get_mut(self.active) {
                            tab.resp = None;
                        }
                        ui.close_menu();
                    }
                    if ui.button("Format body as JSON").clicked() {
                        let idx = self.active;
                        if let Some(tab) = self.tabs.get_mut(idx) {
                            tab.req.body = pretty_body(&tab.req.body);
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Clear history…").clicked() {
                        let count = self.ws.history.len();
                        self.dialog = Some(Dialog::Confirm {
                            title: "Clear history".to_string(),
                            message: format!("Remove all {count} recorded responses?"),
                            pending: Pending::ClearHistory,
                        });
                        ui.close_menu();
                    }
                });
                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.sidebar_visible, "Sidebar");
                    ui.label("Theme:");
                    for option in Theme::ALL {
                        if ui
                            .selectable_label(self.ws.settings.theme == option, option.label())
                            .clicked()
                        {
                            self.ws.settings.theme = option;
                            self.dirty = true;
                        }
                    }
                    ui.separator();
                    ui.checkbox(&mut self.ws.settings.pretty_by_default, "Pretty-print JSON by default");
                    if ui.button("Settings…").clicked() {
                        self.dialog = Some(Dialog::Settings);
                        ui.close_menu();
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("About & shortcuts").clicked() {
                        self.dialog = Some(Dialog::About);
                        ui.close_menu();
                    }
                });
            });
        });

        egui::TopBottomPanel::top("requestbar").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let mut kind = self
                    .tabs
                    .get(self.active)
                    .map(|t| t.req.method_index())
                    .unwrap_or(0);
                egui::ComboBox::from_id_salt("method_combo")
                    .selected_text(METHODS[kind].to_string())
                    .width(100.0)
                    .show_ui(ui, |ui| {
                        for (i, m) in METHODS.iter().enumerate() {
                            ui.selectable_value(&mut kind, i, *m);
                        }
                    });
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.req.set_method(kind);
                }

                let mut url = self
                    .tabs
                    .get(self.active)
                    .map(|t| t.req.url.clone())
                    .unwrap_or_default();
                let available = ui.available_width() - 130.0;
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut url)
                        .hint_text("https://api.example.com/v1/things?limit=20")
                        .desired_width(available.max(120.0))
                        .font(egui::TextStyle::Monospace),
                );
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.req.url = url;
                }
                if resp.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.send_active();
                }
                let sending = self
                    .tabs
                    .get(self.active)
                    .map(|t| t.sending)
                    .unwrap_or(false);
                let label = if sending { "Sending…" } else { "Send" };
                if ui
                    .add_enabled(
                        !sending,
                        egui::Button::new(label).min_size(egui::vec2(92.0, 24.0)),
                    )
                    .on_hover_text("Ctrl+Enter or F5")
                    .clicked()
                {
                    self.send_active();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Environment:");
                let names: Vec<String> = self
                    .ws
                    .environments
                    .iter()
                    .map(|e| e.name.clone())
                    .collect();
                let current = self
                    .ws
                    .active_environment
                    .clone()
                    .unwrap_or_else(|| "No environment".to_string());
                let mut pick: Option<String> = Some(current.clone());
                egui::ComboBox::from_id_salt("env_combo")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut pick, None, "No environment");
                        for n in &names {
                            ui.selectable_value(&mut pick, Some(n.clone()), n);
                        }
                    });
                if pick.is_some() && pick != self.ws.active_environment {
                    self.ws.active_environment = pick;
                    self.dirty = true;
                }
                ui.separator();
                let unresolved: Vec<String> = {
                    let mut out = Vec::new();
                    if let Some(tab) = self.tabs.get(self.active) {
                        for text in [&tab.req.url, &tab.req.body] {
                            for name in self.ws.unresolved(text) {
                                if !out.contains(&name) {
                                    out.push(name);
                                }
                            }
                        }
                    }
                    out
                };
                if unresolved.is_empty() {
                    ui.label("all {{variables}} resolved");
                } else {
                    ui.colored_label(
                        self.palette.warn,
                        format!("undefined: {}", unresolved.join(", ")),
                    );
                }
            });
            ui.add_space(3.0);
        });

        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut close: Option<usize> = None;
                    for i in 0..self.tabs.len() {
                        let title = self.tabs[i].req.title();
                        let sending = self.tabs[i].sending;
                        let prefix = if sending { "● " } else { "" };
                        if ui
                            .selectable_label(self.active == i, format!("{prefix}{title}"))
                            .clicked()
                        {
                            self.active = i;
                        }
                        if self.tabs.len() > 1
                            && ui
                                .small_button("✕")
                                .on_hover_text("Close tab (Ctrl+W)")
                                .clicked()
                        {
                            close = Some(i);
                            break;
                        }
                        ui.separator();
                    }
                    if let Some(i) = close {
                        self.close_tab(i);
                    }
                    if ui
                        .button("+")
                        .on_hover_text("New request tab (Ctrl+T)")
                        .clicked()
                    {
                        self.open_tab(blank_request());
                    }
                });
            });
        });

        let show_response = self
            .tabs
            .get(self.active)
            .map(|t| t.resp.is_some() || t.sending)
            .unwrap_or(false);
        if show_response {
            egui::TopBottomPanel::bottom("response")
                .resizable(true)
                .default_height(340.0)
                .show(ctx, |ui| {
                    self.response_panel(ui, ctx);
                });
        }

        if self.sidebar_visible {
            egui::SidePanel::left("sidebar")
                .resizable(true)
                .default_width(300.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        for (label, target) in [
                            ("Collections", Sidebar::Collections),
                            ("Environments", Sidebar::Environments),
                            ("History", Sidebar::History),
                        ] {
                            if ui
                                .selectable_label(self.sidebar == target, label)
                                .clicked()
                            {
                                self.sidebar = target;
                            }
                        }
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| match self.sidebar {
                            Sidebar::Collections => self.collections_panel(ui),
                            Sidebar::Environments => self.environments_panel(ui),
                            Sidebar::History => self.history_panel(ui),
                        });
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.request_panel(ui);
                });
        });

        self.render_dialogs(ctx);
    }
}

impl App {
    fn request_panel(&mut self, ui: &mut egui::Ui) {
        let idx = self.active;
        let Some(tab) = self.tabs.get_mut(idx) else {
            ui.label("No request selected.");
            return;
        };

        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.add(
                egui::TextEdit::singleline(&mut tab.req.name)
                    .hint_text("My request")
                    .desired_width(220.0),
            );
            ui.separator();
            for (label, view, count) in [
                ("Params", ReqView::Params, tab.req.params.len()),
                ("Headers", ReqView::Headers, tab.req.headers.len()),
            ] {
                let text = if count > 0 {
                    format!("{label} ({count})")
                } else {
                    label.to_string()
                };
                if ui.selectable_label(tab.req_view == view, text).clicked() {
                    tab.req_view = view;
                }
            }
            for (label, view) in [
                ("Body", ReqView::Body),
                ("Auth", ReqView::Auth),
                ("Settings", ReqView::Settings),
            ] {
                if ui.selectable_label(tab.req_view == view, label).clicked() {
                    tab.req_view = view;
                }
            }
        });
        ui.separator();

        match tab.req_view {
            ReqView::Params => {
                ui.horizontal(|ui| {
                    if ui.button("+ Add param").clicked() {
                        tab.req.params.push(Kv::new("", ""));
                    }
                    if ui
                        .button("Split query out of URL")
                        .on_hover_text("Move ?a=1&b=2 from the URL into this grid")
                        .clicked()
                    {
                        pull_query_into_params(&mut tab.req.url, &mut tab.req.params);
                    }
                    if ui.button("Clear").clicked() {
                        tab.req.params.clear();
                    }
                });
                kv_grid(ui, "params_grid", &mut tab.req.params);
                let composed = model::compose_url(&tab.req.url, &tab.req.params);
                if composed != tab.req.url {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("Request will go to: {composed}"))
                            .small()
                            .color(self.palette.info),
                    );
                }
            }
            ReqView::Headers => {
                ui.horizontal(|ui| {
                    if ui.button("+ Add header").clicked() {
                        tab.req.headers.push(Kv::new("", ""));
                    }
                    ui.label("Presets:");
                    for (k, v) in [
                        ("Content-Type", "application/json"),
                        ("Accept", "application/json"),
                        ("User-Agent", "api-checker/1.0"),
                        ("Cache-Control", "no-cache"),
                    ] {
                        if ui.small_button(k).clicked() {
                            tab.req.headers.push(Kv::new(k, v));
                        }
                    }
                    if ui.button("Clear").clicked() {
                        tab.req.headers.clear();
                    }
                });
                kv_grid(ui, "headers_grid", &mut tab.req.headers);
            }
            ReqView::Body => {
                if tab.req.is_bodyless() {
                    ui.colored_label(
                        self.palette.warn,
                        format!("{} requests send no body.", tab.req.method),
                    );
                }
                ui.horizontal(|ui| {
                    for (label, mode) in [
                        ("none", BodyMode::None),
                        ("raw / JSON", BodyMode::Raw),
                        ("x-www-form-urlencoded", BodyMode::UrlEncoded),
                    ] {
                        if ui
                            .selectable_label(tab.req.body_mode == mode, label)
                            .clicked()
                        {
                            tab.req.body_mode = mode;
                        }
                    }
                });
                ui.add_space(4.0);
                match tab.req.body_mode {
                    BodyMode::None => {
                        ui.label("No body will be sent.");
                    }
                    BodyMode::Raw => {
                        ui.horizontal(|ui| {
                            if ui.button("Format JSON").clicked() {
                                tab.req.body = pretty_body(&tab.req.body);
                            }
                            if ui.button("Minify").clicked() {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&tab.req.body)
                                {
                                    tab.req.body = v.to_string();
                                }
                            }
                            if ui.button("Sample").clicked() {
                                tab.req.body =
                                    "{\n  \"name\": \"api-checker\",\n  \"active\": true\n}".to_string();
                            }
                        });
                        ui.add(
                            egui::TextEdit::multiline(&mut tab.req.body)
                                .desired_rows(10)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .hint_text("{ \"key\": \"value\" }"),
                        );
                    }
                    BodyMode::UrlEncoded => {
                        if ui.button("+ Add field").clicked() {
                            tab.req.form.push(Kv::new("", ""));
                        }
                        kv_grid(ui, "form_grid", &mut tab.req.form);
                    }
                }
            }
            ReqView::Auth => {
                let mut kind = auth_kind(&tab.req.auth);
                egui::ComboBox::from_id_salt("auth_kind")
                    .selected_text(match kind {
                        0 => "No auth",
                        1 => "Bearer token",
                        2 => "Basic auth",
                        _ => "API key",
                    })
                    .show_ui(ui, |ui| {
                        for (i, label) in [
                            "No auth",
                            "Bearer token",
                            "Basic auth",
                            "API key",
                        ]
                        .iter()
                        .enumerate()
                        {
                            ui.selectable_value(&mut kind, i, *label);
                        }
                    });
                if auth_kind(&tab.req.auth) != kind {
                    set_auth_kind(&mut tab.req, kind);
                }
                ui.add_space(6.0);
                match &mut tab.req.auth {
                    model::Auth::NoAuth => {
                        ui.label("Nothing is added to the request.");
                    }
                    model::Auth::Bearer { token } => {
                        ui.horizontal(|ui| {
                            ui.label("Token:");
                            ui.add(
                                egui::TextEdit::singleline(token)
                                    .hint_text("{{token}} or literal")
                                    .desired_width(320.0),
                            );
                        });
                        ui.label("Sent as Authorization: Bearer <token>");
                    }
                    model::Auth::Basic { username, password } => {
                        ui.horizontal(|ui| {
                            ui.label("Username:");
                            ui.add(egui::TextEdit::singleline(username).desired_width(200.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Password:");
                            ui.add(
                                egui::TextEdit::singleline(password)
                                    .password(true)
                                    .desired_width(200.0),
                            );
                        });
                        ui.label("Base64 of user:password, sent as Authorization: Basic …");
                    }
                    model::Auth::ApiKey {
                        key,
                        value,
                        location,
                    } => {
                        ui.horizontal(|ui| {
                            ui.label("Key:");
                            ui.add(egui::TextEdit::singleline(key).desired_width(180.0));
                            ui.label("Value:");
                            ui.add(
                                egui::TextEdit::singleline(value)
                                    .desired_width(220.0)
                                    .password(true),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("Add as:");
                            for (label, variant) in [
                                ("header", model::ApiKeyIn::Header),
                                ("query param", model::ApiKeyIn::Query),
                            ] {
                                if ui
                                    .selectable_label(*location == variant, label)
                                    .clicked()
                                {
                                    *location = variant;
                                }
                            }
                        });
                    }
                }
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Values support {{variables}} from the active environment.")
                        .italics()
                        .small(),
                );
            }
            ReqView::Settings => {
                ui.horizontal(|ui| {
                    ui.label("Timeout (seconds):");
                    ui.add(egui::DragValue::new(&mut tab.req.settings.timeout_secs).range(1..=600));
                });
                ui.checkbox(&mut tab.req.settings.follow_redirects, "Follow redirects");
                if tab.req.settings.follow_redirects {
                    ui.horizontal(|ui| {
                        ui.label("Max redirects:");
                        ui.add(egui::DragValue::new(&mut tab.req.settings.max_redirects).range(1..=50));
                    });
                }
                ui.checkbox(
                    &mut tab.req.settings.verify_tls,
                    "Verify TLS certificate",
                )
                .on_hover_text("Disable only for local self-signed test servers");
                ui.checkbox(&mut tab.req.settings.cookie_jar, "Share cookies across requests");
                ui.add_space(6.0);
                ui.label("Comment (saved with the request):");
                ui.add(
                    egui::TextEdit::multiline(&mut tab.req.comment)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                );
            }
        }
    }

    fn response_panel(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let idx = self.active;
        let sending = self.tabs.get(idx).map(|t| t.sending).unwrap_or(false);
        let resp = self.tabs.get(idx).and_then(|t| t.resp.clone());
        let req = self
            .tabs
            .get(idx)
            .map(|t| t.req.clone())
            .unwrap_or_else(blank_request);
        let vars = self.ws.vars();
        let mut view = self
            .tabs
            .get(idx)
            .map(|t| t.resp_view)
            .unwrap_or(RespView::Pretty);
        let mut lang = self.tabs.get(idx).map(|t| t.code_lang).unwrap_or(0);
        let mut pretty = self.ws.settings.pretty_by_default;
        let mut copy: Option<String> = None;

        ui.horizontal(|ui| {
            ui.heading("Response");
            if sending && resp.is_none() {
                ui.spinner();
                ui.label("waiting for the server…");
            }
            if let Some(resp) = &resp {
                ui.label(
                    RichText::new(format!("{} {}", resp.status, resp.status_text))
                        .color(self.palette.status(if resp.error.is_some() { None } else { Some(resp.status) }))
                        .strong(),
                );
                ui.separator();
                ui.label(format!("{} ms", resp.elapsed_ms));
                ui.label(human_size(resp.body_bytes));
                if let Some(ct) = &resp.content_type {
                    ui.label(ct.clone());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button("Copy body")
                        .on_hover_text("Copy the raw response body")
                        .clicked()
                    {
                        copy = Some(resp.body.clone());
                    }
                    if ui.button("Copy URL").clicked() {
                        copy = Some(resp.final_url.clone());
                    }
                });
            }
        });

        if let Some(resp) = &resp {
            ui.horizontal(|ui| {
                for (label, v, count) in [
                    ("Pretty", RespView::Pretty, 0usize),
                    ("Raw", RespView::Raw, 0),
                    ("Headers", RespView::Headers, resp.headers.len()),
                    ("Cookies", RespView::Cookies, resp.cookies.len()),
                    ("Code", RespView::Code, 0),
                    ("Meta", RespView::Meta, 0),
                ] {
                    let text = if count > 0 {
                        format!("{label} ({count})")
                    } else {
                        label.to_string()
                    };
                    if ui.selectable_label(view == v, text).clicked() {
                        view = v;
                    }
                }
                ui.separator();
                ui.checkbox(&mut pretty, "pretty");
            });
            if let Some(err) = &resp.error {
                ui.colored_label(
                    self.palette.err,
                    RichText::new(format!("Request failed: {err}")).strong(),
                );
            }
            ui.separator();

            egui::ScrollArea::vertical()
                .id_salt("resp_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| match view {
                    RespView::Pretty => {
                        let text = if pretty {
                            pretty_body(&resp.body)
                        } else {
                            resp.body.clone()
                        };
                        if text.is_empty() {
                            ui.label("(empty body)");
                        } else {
                            let mut shown = text.as_str();
                            ui.add(
                                egui::TextEdit::multiline(&mut shown)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY)
                                    .interactive(false),
                            );
                        }
                    }
                    RespView::Raw => {
                        let mut shown = resp.body.as_str();
                        ui.add(
                            egui::TextEdit::multiline(&mut shown)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .interactive(false),
                        );
                    }
                    RespView::Headers => {
                        egui::Grid::new("resp_headers")
                            .num_columns(2)
                            .spacing([16.0, 3.0])
                            .striped(true)
                            .show(ui, |ui| {
                                for (k, v) in &resp.headers {
                                    ui.monospace(k);
                                    ui.label(v);
                                    ui.end_row();
                                }
                            });
                    }
                    RespView::Cookies => {
                        if resp.cookies.is_empty() {
                            ui.label("This response set no cookies.");
                        } else {
                            egui::Grid::new("resp_cookies")
                                .num_columns(2)
                                .spacing([16.0, 3.0])
                                .striped(true)
                                .show(ui, |ui| {
                                    for (k, v) in &resp.cookies {
                                        ui.monospace(k);
                                        ui.label(v);
                                        ui.end_row();
                                    }
                                });
                            ui.label(
                                RichText::new("Cookies are kept in a shared jar for later requests.")
                                    .small()
                                    .italics(),
                            );
                        }
                    }
                    RespView::Code => {
                        ui.horizontal(|ui| {
                            for (i, name) in CODE_LANGS.iter().enumerate() {
                                if ui.selectable_label(lang == i, *name).clicked() {
                                    lang = i;
                                }
                            }
                        });
                        let snippet = match lang {
                            1 => curl::curl_windows(&req, &vars),
                            2 => curl::powershell(&req, &vars),
                            3 => curl::python(&req, &vars),
                            4 => curl::js_fetch(&req, &vars),
                            _ => curl::curl_bash(&req, &vars),
                        };
                        let mut snippet = snippet.as_str();
                        if ui.button("Copy snippet").clicked() {
                            copy = Some(snippet.to_string());
                        }
                        ui.add_space(4.0);
                        ui.add(
                            egui::TextEdit::multiline(&mut snippet)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .interactive(false),
                        );
                    }
                    RespView::Meta => {
                        let rows: Vec<(String, String)> = vec![
                            (
                                "Status".to_string(),
                                format!("{} {}", resp.status, resp.status_text),
                            ),
                            ("Time".to_string(), format!("{} ms", resp.elapsed_ms)),
                            ("Size".to_string(), human_size(resp.body_bytes)),
                            (
                                "Content-Type".to_string(),
                                resp.content_type.clone().unwrap_or_else(|| "-".to_string()),
                            ),
                            (
                                "Final URL".to_string(),
                                if resp.final_url.is_empty() {
                                    model::compose_url(&req.url, &req.params)
                                } else {
                                    resp.final_url.clone()
                                },
                            ),
                            (
                                "Headers sent".to_string(),
                                req.headers.iter().filter(|h| h.enabled).count().to_string(),
                            ),
                            ("Params".to_string(), req.params.len().to_string()),
                            (
                                "Redirects".to_string(),
                                if req.settings.follow_redirects {
                                    format!("followed, max {}", req.settings.max_redirects)
                                } else {
                                    "not followed".to_string()
                                },
                            ),
                            ("TLS verify".to_string(), req.settings.verify_tls.to_string()),
                            (
                                "Timeout".to_string(),
                                format!("{} s", req.settings.timeout_secs),
                            ),
                            (
                                "Auth".to_string(),
                                match auth_kind(&req.auth) {
                                    0 => "none".to_string(),
                                    1 => "bearer".to_string(),
                                    2 => "basic".to_string(),
                                    _ => "api key".to_string(),
                                },
                            ),
                            (
                                "Cookies set".to_string(),
                                resp.cookies.len().to_string(),
                            ),
                        ];
                        egui::Grid::new("resp_meta")
                            .num_columns(2)
                            .spacing([16.0, 3.0])
                            .striped(true)
                            .show(ui, |ui| {
                                for (k, v) in &rows {
                                    ui.strong(k);
                                    ui.label(v);
                                    ui.end_row();
                                }
                            });
                        let unresolved = self.ws.unresolved(&req.url);
                        if !unresolved.is_empty() {
                            ui.colored_label(
                                self.palette.warn,
                                format!("undefined variables: {}", unresolved.join(", ")),
                            );
                        }
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label("Write body to file:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.save_body_path)
                                    .desired_width(320.0),
                            );
                            let path = self.save_body_path.clone();
                            if ui.button("Save").clicked() {
                                let text = if pretty {
                                    pretty_body(&resp.body)
                                } else {
                                    resp.body.clone()
                                };
                                match std::fs::write(&path, text) {
                                    Ok(_) => self.notify(format!("Wrote {path}"), false),
                                    Err(e) => self.notify(format!("Could not write {path}: {e}"), true),
                                }
                            }
                        });
                    }
                });
        } else if !sending {
            ui.label("No response yet.");
        }

        if let Some(tab) = self.tabs.get_mut(idx) {
            tab.resp_view = view;
            tab.code_lang = lang;
        }
        if pretty != self.ws.settings.pretty_by_default {
            self.ws.settings.pretty_by_default = pretty;
            self.dirty = true;
        }
        if let Some(text) = copy {
            ctx.copy_text(text);
        }
    }

    fn collections_panel(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let mut to_open: Option<RequestDef> = None;
        let mut confirm: Option<Dialog> = None;
        let mut new_collection = false;
        let mut new_folder_for: Option<usize> = None;
        let mut export_col: Option<usize> = None;

        if ui.button("+ New collection").clicked() {
            new_collection = true;
        }
        ui.add_space(4.0);

        for (ci, col) in self.ws.collections.iter_mut().enumerate() {
            let title = format!("{} · {} request(s)", col.name, col.total_requests());
            ui.push_id(ci, |ui| {
                egui::CollapsingHeader::new(title)
                    .default_open(ci == 0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.small_button("+ folder").clicked() {
                                new_folder_for = Some(ci);
                            }
                            if ui.small_button("Export").clicked() {
                                export_col = Some(ci);
                            }
                            if ui.small_button("Delete").clicked() {
                                confirm = Some(Dialog::Confirm {
                                    title: "Delete collection".to_string(),
                                    message: format!(
                                        "Delete \"{}\" and its {} request(s)?",
                                        col.name,
                                        col.total_requests()
                                    ),
                                    pending: Pending::DeleteCollection(ci),
                                });
                            }
                        });

                        ui.label(RichText::new("Requests").small().italics());
                        let root_count = col.requests.len();
                        for ri in 0..root_count {
                            let req = col.requests[ri].clone();
                            let (pick, del) = ui
                                .horizontal(|ui| {
                                    ui.colored_label(p.method(&req.method), req.method.clone());
                                    let pick = ui.selectable_label(false, req.title());
                                    let del = ui.small_button("✕").on_hover_text("Delete request");
                                    (pick.clicked(), del.clicked())
                                })
                                .inner;
                            if pick {
                                to_open = Some(req.clone());
                            }
                            if del {
                                confirm = Some(Dialog::Confirm {
                                    title: "Delete request".to_string(),
                                    message: format!(
                                        "Remove \"{}\" from {}?",
                                        req.title(),
                                        col.name
                                    ),
                                    pending: Pending::DeleteRequest {
                                        collection: ci,
                                        folder: None,
                                        index: ri,
                                    },
                                });
                            }
                        }

                        ui.add_space(4.0);
                        ui.label(RichText::new("Collection variables").small().italics());
                        let removed = kv_editor(
                            ui,
                            &format!("colvars-{ci}"),
                            &mut col.variables,
                            p,
                        );
                        if let Some(r) = removed {
                            col.variables.remove(r);
                        }
                        if ui.small_button("+ Add variable").clicked() {
                            col.variables.push(Kv::new("", ""));
                        }
                        if !col.description.is_empty() {
                            ui.label(RichText::new(&col.description).weak().small());
                        }

                        ui.add_space(4.0);
                        let folder_count = col.folders.len();
                        for fi in 0..folder_count {
                            let folder_name = col.folders[fi].name.clone();
                            let count = col.folders[fi].requests.len();
                            ui.push_id(format!("folder-{fi}"), |ui| {
                                egui::CollapsingHeader::new(format!("{folder_name} · {count}"))
                                    .show(ui, |ui| {
                                        let mut rename = col.folders[fi].name.clone();
                                        ui.horizontal(|ui| {
                                            ui.label("Name:");
                                            ui.add(
                                                egui::TextEdit::singleline(&mut rename)
                                                    .desired_width(150.0),
                                            );
                                        });
                                        col.folders[fi].name = rename.clone();
                                        let req_count = col.folders[fi].requests.len();
                                        for ri in 0..req_count {
                                            let req = col.folders[fi].requests[ri].clone();
                                            let (pick, del) = ui
                                                .horizontal(|ui| {
                                                    ui.colored_label(
                                                        p.method(&req.method),
                                                        req.method.clone(),
                                                    );
                                                    let pick =
                                                        ui.selectable_label(false, req.title());
                                                    let del = ui.small_button("✕");
                                                    (pick.clicked(), del.clicked())
                                                })
                                                .inner;
                                            if pick {
                                                to_open = Some(req.clone());
                                            }
                                            if del {
                                                confirm = Some(Dialog::Confirm {
                                                    title: "Delete request".to_string(),
                                                    message: format!(
                                                        "Remove \"{}\" from {}?",
                                                        req.title(),
                                                        rename
                                                    ),
                                                    pending: Pending::DeleteRequest {
                                                        collection: ci,
                                                        folder: Some(rename.clone()),
                                                        index: ri,
                                                    },
                                                });
                                            }
                                        }
                                    });
                            });
                        }
                    });
            });
        }

        if new_collection {
            let n = self.ws.collections.len() + 1;
            self.ws
                .collections
                .push(Collection::new(&format!("Collection {n}")));
            self.dirty = true;
        }
        if let Some(ci) = new_folder_for {
            if let Some(col) = self.ws.collections.get_mut(ci) {
                let n = col.folders.len() + 1;
                col.folders.push(collections::Folder {
                    name: format!("Folder {n}"),
                    requests: Vec::new(),
                });
                self.dirty = true;
            }
        }
        if let Some(req) = to_open {
            self.open_tab(req);
        }
        if let Some(ci) = export_col {
            let name = self
                .ws
                .collections
                .get(ci)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            let path = storage::config_path()
                .join(format!("{}.postman_collection.json", name.replace(' ', "_")))
                .display()
                .to_string();
            self.dialog = Some(Dialog::Io {
                mode: IoMode::Export,
                path,
                content: String::new(),
                collection: ci,
            });
        }
        if confirm.is_some() {
            self.dialog = confirm;
        }
    }

    fn environments_panel(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let mut add_env = false;
        let mut delete_env: Option<usize> = None;
        let mut activate: Option<usize> = None;
        let mut add_var: Option<usize> = None;
        let mut del_var: Option<(usize, usize)> = None;
        let mut clear_active = false;

        ui.horizontal(|ui| {
            if ui.button("+ New environment").clicked() {
                add_env = true;
            }
            if ui.button("Use none").clicked() {
                clear_active = true;
            }
        });
        ui.label(
            RichText::new("Write {{name}} in a URL, header or body to substitute these values.")
                .small()
                .color(p.muted),
        );
        ui.separator();

        for i in 0..self.ws.environments.len() {
            let name_before = self.ws.environments[i].name.clone();
            let is_active = self.ws.active_environment.as_deref() == Some(name_before.as_str());
            let mut name = name_before.clone();
            let (activate_click, delete_click) = ui
                .horizontal(|ui| {
                    let act = ui
                        .selectable_label(
                            is_active,
                            if is_active { "● active" } else { "○ use" },
                        )
                        .clicked();
                    ui.add(
                        egui::TextEdit::singleline(&mut name)
                            .hint_text("environment name")
                            .desired_width(112.0),
                    );
                    let del = ui
                        .small_button("✕")
                        .on_hover_text("Delete environment")
                        .clicked();
                    (act, del)
                })
                .inner;
            if activate_click {
                activate = Some(i);
            }
            if delete_click {
                delete_env = Some(i);
            }
            if name != name_before {
                self.ws.environments[i].name = name.clone();
                if is_active {
                    self.ws.active_environment = Some(name);
                }
                self.dirty = true;
            }

            let removed = kv_editor(
                ui,
                &format!("env-{i}"),
                &mut self.ws.environments[i].variables,
                p,
            );
            if let Some(r) = removed {
                del_var = Some((i, r));
            }
            if ui.small_button("+ Add variable").clicked() {
                add_var = Some(i);
            }
            ui.add_space(6.0);
            ui.separator();
        }

        if add_env {
            let n = self.ws.environments.len() + 1;
            self.ws.environments.push(Environment {
                name: format!("env{n}"),
                variables: vec![Kv::new("", "")],
            });
            self.dirty = true;
        }
        if clear_active {
            self.ws.active_environment = None;
            self.dirty = true;
        }
        if let Some(i) = activate {
            self.ws.active_environment = Some(self.ws.environments[i].name.clone());
            self.dirty = true;
        }
        if let Some(i) = add_var {
            if let Some(env) = self.ws.environments.get_mut(i) {
                env.variables.push(Kv::new("", ""));
                self.dirty = true;
            }
        }
        if let Some((i, r)) = del_var {
            if let Some(env) = self.ws.environments.get_mut(i) {
                if r < env.variables.len() {
                    env.variables.remove(r);
                    self.dirty = true;
                }
            }
        }
        if let Some(i) = delete_env {
            self.dialog = Some(Dialog::Confirm {
                title: "Delete environment".to_string(),
                message: format!(
                    "Delete \"{}\" and all of its variables?",
                    self.ws
                        .environments
                        .get(i)
                        .map(|e| e.name.clone())
                        .unwrap_or_default()
                ),
                pending: Pending::DeleteEnvironment(i),
            });
        }
    }

    fn history_panel(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let mut open: Option<usize> = None;
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.history_filter)
                    .hint_text("filter by url or method")
                    .desired_width(ui.available_width()),
            );
        });
        ui.horizontal(|ui| {
            ui.label(format!("{} entries", self.ws.history.len()));
            if ui.small_button("Clear").clicked() {
                let count = self.ws.history.len();
                self.dialog = Some(Dialog::Confirm {
                    title: "Clear history".to_string(),
                    message: format!("Remove all {count} recorded responses?"),
                    pending: Pending::ClearHistory,
                });
            }
        });
        ui.separator();
        let filter = self.history_filter.to_lowercase();
        for (i, h) in self.ws.history.iter().enumerate() {
            if !filter.is_empty()
                && !h.url.to_lowercase().contains(&filter)
                && !h.method.to_lowercase().contains(&filter)
            {
                continue;
            }
            let status = match h.status {
                Some(s) => s.to_string(),
                None => "ERR".to_string(),
            };
            let url = h.url.clone();
            let at = h.at.clone();
            let timing = format!("{} ms · {}", h.elapsed_ms, human_size(h.body_bytes));
            let clicked = ui
                .vertical(|ui| {
                    let picked = ui
                        .horizontal(|ui| {
                            ui.colored_label(
                                p.status(h.status),
                                format!("{} {}", h.method, status),
                            );
                            ui.selectable_label(false, RichText::new(url.as_str()).small())
                                .clicked()
                        })
                        .inner;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(at.as_str()).weak().small());
                        ui.label(RichText::new(timing.as_str()).small());
                    });
                    picked
                })
                .inner;
            if clicked {
                open = Some(i);
            }
            ui.separator();
        }
        if let Some(i) = open {
            if let Some(req) = self.ws.history.get(i) {
                let mut req = req.request.clone();
                if req.name == "New request" || req.name.is_empty() {
                    req.name = format!("{} · rerun", req.method);
                }
                self.open_tab(req);
            }
        }
    }

    fn render_dialogs(&mut self, ctx: &Context) {
        let Some(current) = self.dialog.take() else {
            return;
        };
        let mut keep: Option<Dialog> = None;
        match current {
            Dialog::Save {
                mut name,
                mut collection,
                mut folder,
            } => {
                let mut visible = true;
                let mut done = false;
                egui::Window::new("Save request to collection")
                    .open(&mut visible)
                    .resizable([true, true])
                    .collapsible(false)
                    .default_width(420.0)
                    .show(ctx, |ui| {
                        let current_name = self
                            .tabs
                            .get(self.active)
                            .map(|t| t.req.name.clone())
                            .unwrap_or_default();
                        if name.is_empty() {
                            name = current_name;
                        }
                        ui.horizontal(|ui| {
                            ui.label("Name:");
                            ui.add(
                                egui::TextEdit::singleline(&mut name).desired_width(280.0),
                            );
                        });
                        let names: Vec<String> = self
                            .ws
                            .collections
                            .iter()
                            .map(|c| c.name.clone())
                            .collect();
                        if collection >= names.len() {
                            collection = 0;
                        }
                        ui.horizontal(|ui| {
                            ui.label("Collection:");
                            egui::ComboBox::from_id_salt("save_col")
                                .selected_text(names.get(collection).cloned().unwrap_or_default())
                                .show_ui(ui, |ui| {
                                    for (i, n) in names.iter().enumerate() {
                                        ui.selectable_value(&mut collection, i, n);
                                    }
                                });
                        });
                        let folders: Vec<String> = self
                            .ws
                            .collections
                            .get(collection)
                            .map(|c| c.folder_names())
                            .unwrap_or_else(|| vec!["<root>".to_string()]);
                        ui.horizontal(|ui| {
                            ui.label("Folder:");
                            egui::ComboBox::from_id_salt("save_folder")
                                .selected_text(folder.clone())
                                .show_ui(ui, |ui| {
                                    for f in &folders {
                                        ui.selectable_value(&mut folder, f.clone(), f);
                                    }
                                });
                        });
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("Save").clicked() {
                                self.save_request_to(collection, &folder, &name);
                                done = true;
                            }
                            if ui.button("Cancel").clicked() {
                                done = true;
                            }
                        });
                    });
                if visible && !done {
                    keep = Some(Dialog::Save {
                        name,
                        collection,
                        folder,
                    });
                }
            }
            Dialog::Io {
                mode,
                mut path,
                mut content,
                mut collection,
            } => {
                let mut visible = true;
                let mut done = false;
                let title = match mode {
                    IoMode::Import => "Import Postman collection",
                    IoMode::Export => "Export collection as Postman JSON",
                };
                egui::Window::new(title)
                    .open(&mut visible)
                    .resizable([true, true])
                    .collapsible(false)
                    .default_width(560.0)
                    .show(ctx, |ui| {
                        match mode {
                            IoMode::Import => {
                                ui.label("Paste a Postman v2.1 collection export, or give a file path:");
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut path)
                                            .hint_text(r"C:\Users\you\Downloads\collection.json")
                                            .desired_width(360.0),
                                    );
                                    if ui.button("Read file").clicked() {
                                        match std::fs::read_to_string(&path) {
                                            Ok(text) => content = text,
                                            Err(e) => {
                                                self.notify(format!("Cannot read {path}: {e}"), true)
                                            }
                                        }
                                    }
                                });
                                ui.add_space(4.0);
                                ui.add(
                                    egui::TextEdit::multiline(&mut content)
                                        .desired_rows(12)
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(f32::INFINITY),
                                );
                                ui.horizontal(|ui| {
                                    if ui.button("Import").clicked() {
                                        if !content.trim().is_empty() {
                                            let label = if path.is_empty() {
                                                "pasted JSON".to_string()
                                            } else {
                                                path.clone()
                                            };
                                            self.import_from_text(&content, &label);
                                            done = true;
                                        } else if !path.trim().is_empty() {
                                            self.import_from_path(&path);
                                            done = true;
                                        } else {
                                            self.notify("Nothing to import.", true);
                                        }
                                    }
                                    if ui.button("Cancel").clicked() {
                                        done = true;
                                    }
                                });
                            }
                            IoMode::Export => {
                                let names: Vec<String> = self
                                    .ws
                                    .collections
                                    .iter()
                                    .map(|c| c.name.clone())
                                    .collect();
                                if collection >= names.len() {
                                    collection = 0;
                                }
                                ui.horizontal(|ui| {
                                    ui.label("Collection:");
                                    egui::ComboBox::from_id_salt("export_col")
                                        .selected_text(
                                            names.get(collection).cloned().unwrap_or_default(),
                                        )
                                        .show_ui(ui, |ui| {
                                            for (i, n) in names.iter().enumerate() {
                                                ui.selectable_value(&mut collection, i, n);
                                            }
                                        });
                                });
                                ui.horizontal(|ui| {
                                    ui.label("File:");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut path)
                                            .desired_width(340.0),
                                    );
                                });
                                if let Some(col) = self.ws.collections.get(collection) {
                                    let n = col.total_requests();
                                    ui.label(format!("{n} request(s) will be written."));
                                }
                                if ui.button("Preview").clicked() {
                                    if let Some(col) = self.ws.collections.get(collection) {
                                        content = serde_json::to_string_pretty(
                                            &collections::to_postman_json(col),
                                        )
                                        .unwrap_or_default();
                                    }
                                }
                                if !content.is_empty() {
                                    let short: String = content.chars().take(2000).collect();
                                    let mut preview = short.as_str();
                                    ui.add(
                                        egui::TextEdit::multiline(&mut preview)
                                            .desired_rows(8)
                                            .font(egui::TextStyle::Monospace)
                                            .desired_width(f32::INFINITY)
                                            .interactive(false),
                                    );
                                }
                                ui.horizontal(|ui| {
                                    if ui.button("Export").clicked() {
                                        self.export_collection(collection, &path);
                                        done = true;
                                    }
                                    if ui.button("Cancel").clicked() {
                                        done = true;
                                    }
                                });
                            }
                        }
                    });
                if visible && !done {
                    keep = Some(Dialog::Io {
                        mode,
                        path,
                        content,
                        collection,
                    });
                }
            }
            Dialog::Confirm {
                title,
                message,
                pending,
            } => {
                let mut visible = true;
                let mut done = false;
                egui::Window::new(title.clone())
                    .open(&mut visible)
                    .collapsible(false)
                    .resizable([true, true])
                    .default_width(400.0)
                    .show(ctx, |ui| {
                        ui.label(message.clone());
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button("Yes").clicked() {
                                self.do_pending(pending.clone());
                                done = true;
                            }
                            if ui.button("No").clicked() {
                                done = true;
                            }
                        });
                    });
                if visible && !done {
                    keep = Some(Dialog::Confirm {
                        title,
                        message,
                        pending,
                    });
                }
            }
            Dialog::Settings => {
                let mut visible = true;
                let mut done = false;
                egui::Window::new("Settings")
                    .open(&mut visible)
                    .collapsible(false)
                    .resizable([true, true])
                    .default_width(480.0)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("HTTP/HTTPS proxy:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.ws.settings.proxy_url)
                                    .hint_text("http://127.0.0.1:8080 (blank = none)")
                                    .desired_width(260.0),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("Default timeout (s):");
                            ui.add(
                                egui::DragValue::new(&mut self.ws.settings.default_timeout_secs)
                                    .range(1..=600),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("Theme:");
                            egui::ComboBox::from_id_salt("theme_combo")
                                .selected_text(self.ws.settings.theme.label())
                                .show_ui(ui, |ui| {
                                    for option in Theme::ALL {
                                        if ui
                                            .selectable_value(&mut self.ws.settings.theme, option, option.label())
                                            .clicked()
                                        {
                                            self.dirty = true;
                                        }
                                    }
                                });
                        });
                        ui.checkbox(&mut self.ws.settings.pretty_by_default, "Pretty-print JSON");
                        ui.separator();
                        ui.label(format!(
                            "Workspace file: {}",
                            storage::state_file().display()
                        ));
                        ui.horizontal(|ui| {
                            if ui.button("Save now").clicked() {
                                self.autosave(true);
                                let p = storage::state_file().display().to_string();
                                self.notify(format!("Workspace written to {p}"), false);
                            }
                            if ui.button("Close").clicked() {
                                done = true;
                            }
                        });
                    });
                if visible && !done {
                    keep = Some(Dialog::Settings);
                }
            }
            Dialog::About => {
                let mut visible = true;
                let mut done = false;
                egui::Window::new(APP_NAME)
                    .open(&mut visible)
                    .collapsible(false)
                    .resizable([true, true])
                    .default_width(460.0)
                    .show(ctx, |ui| {
                        ui.label(
                            "A Postman-style HTTP client: collections, environments, variables, auth, \
                             params, bodies, cookie jar, response views and code generation.",
                        );
                        ui.add_space(6.0);
                        ui.strong("Shortcuts");
                        for (k, v) in [
                            ("Ctrl+Enter / F5", "Send the active request"),
                            ("Ctrl+T", "New request tab"),
                            ("Ctrl+W", "Close the active tab"),
                            ("Ctrl+S", "Save request into a collection"),
                            ("Ctrl+Shift+S", "Save the workspace to disk"),
                            ("Enter", "Send while the URL field is focused"),
                        ] {
                            ui.horizontal(|ui| {
                                ui.monospace(k);
                                ui.label(v);
                            });
                        }
                        ui.add_space(6.0);
                        ui.label(
                            "Not supported: JS pre-request/test scripts, WebSocket, gRPC, mock servers, cloud sync.",
                        );
                        if ui.button("Close").clicked() {
                            done = true;
                        }
                    });
                if visible && !done {
                    keep = Some(Dialog::About);
                }
            }
        }
        self.dialog = keep;
    }
}

fn run_selftest(url: &str) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let vars = std::collections::BTreeMap::new();
    let jar = Arc::new(Jar::default());

    let get = RequestDef {
        name: "selftest get".to_string(),
        method: "GET".to_string(),
        url: url.to_string(),
        headers: vec![Kv::new("Accept", "application/json")],
        ..Default::default()
    };
    let resp = rt.block_on(send::perform(&get, &vars, jar.clone(), None));
    report("GET", &resp);
    if resp.error.is_some() {
        return Err("GET failed".to_string());
    }

    let mut bearer = get.clone();
    bearer.method = "POST".to_string();
    bearer.url = format!("{}/post", url.trim_end_matches('/'));
    bearer.body_mode = BodyMode::Raw;
    bearer.body = "{ \"hello\": \"world\" }".to_string();
    bearer.auth = model::Auth::Bearer {
        token: "selftest-token".to_string(),
    };
    let resp = rt.block_on(send::perform(&bearer, &vars, jar.clone(), None));
    report("POST + bearer", &resp);

    let mut missing = get.clone();
    missing.url = format!("{}/status/404", url.trim_end_matches('/'));
    let resp = rt.block_on(send::perform(&missing, &vars, jar.clone(), None));
    report("404 status", &resp);
    if resp.status != 404 {
        return Err(format!("expected 404, got {}", resp.status));
    }

    let mut cookie = get.clone();
    cookie.url = format!("{}/cookies/set?session=abc123", url.trim_end_matches('/'));
    cookie.settings.max_redirects = 5;
    let resp = rt.block_on(send::perform(&cookie, &vars, jar.clone(), None));
    report("cookie set + redirect", &resp);

    let mut bad = get.clone();
    bad.url = "https://this-host-does-not-exist.invalid/".to_string();
    bad.settings.timeout_secs = 10;
    let resp = rt.block_on(send::perform(&bad, &vars, jar, None));
    match &resp.error {
        Some(e) => println!(
            "unreachable host -> reported as error in {} ms: {}",
            resp.elapsed_ms,
            &e.chars().take(90).collect::<String>()
        ),
        None => return Err("unreachable host did not surface an error".to_string()),
    }

    if substitute("{{x}}", &vars) != "{{x}}" {
        return Err("variable substitution regressed".to_string());
    }
    Ok(())
}

fn report(label: &str, resp: &ResponseBundle) {
    let head: String = resp.body.chars().take(120).collect();
    println!(
        "{label}: {} {} · {} ms · {} · cookies {} · {head}",
        resp.status,
        resp.status_text,
        resp.elapsed_ms,
        human_size(resp.body_bytes),
        resp.cookies.len(),
    );
    if let Some(err) = &resp.error {
        println!("  error: {err}");
    }
}

/// The release build is a GUI-subsystem binary with no console of its own, so
/// attach to the parent terminal to let `--selftest` print.
#[cfg(windows)]
fn attach_parent_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(dwProcessId: u32) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

#[cfg(not(windows))]
fn attach_parent_console() {}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--selftest") {
        attach_parent_console();
        let url = args
            .get(pos + 1)
            .cloned()
            .unwrap_or_else(|| "https://httpbin.org".to_string());
        return match run_selftest(&url) {
            Ok(()) => Ok(()),
            Err(e) => {
                eprintln!("selftest failed: {e}");
                std::process::exit(1);
            }
        };
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1240.0, 820.0])
            .with_min_inner_size([900.0, 620.0])
            .with_title(APP_NAME),
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.update_ui(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> RequestDef {
        RequestDef {
            name: "List things".to_string(),
            method: "GET".to_string(),
            url: "https://api.test/v1/things".to_string(),
            params: vec![Kv::new("limit", "20")],
            headers: vec![Kv::new("X-Trace", "{{trace_id}}")],
            body: String::new(),
            body_mode: BodyMode::None,
            form: vec![],
            auth: model::Auth::Bearer {
                token: "{{token}}".to_string(),
            },
            settings: Default::default(),
            comment: String::new(),
        }
    }

    fn vars() -> std::collections::BTreeMap<String, String> {
        [
            ("token".to_string(), "secret123".to_string()),
            ("trace_id".to_string(), "abc".to_string()),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn substitutes_known_and_keeps_unknown() {
        let v = vars();
        assert_eq!(substitute("{{token}}", &v), "secret123");
        assert_eq!(substitute("Bearer  {{token}} ", &v), "Bearer  secret123 ");
        assert_eq!(substitute("{{nope}}", &v), "{{nope}}");
        assert_eq!(substitute("no braces", &v), "no braces");
        assert_eq!(substitute("{{unterminated", &v), "{{unterminated");
    }

    #[test]
    fn composes_url_with_params() {
        let req = sample();
        let url = model::compose_url(&req.url, &req.params);
        assert_eq!(url, "https://api.test/v1/things?limit=20");
    }

    #[test]
    fn composes_url_keeps_existing_query_and_skips_duplicates() {
        let params = vec![Kv::new("a", "1"), Kv::new("b", "2"), Kv {
            key: "b".to_string(),
            value: "ignored".to_string(),
            enabled: true,
            description: String::new(),
        }];
        let url = model::compose_url("https://x.test/p?a=1", &params);
        assert_eq!(url, "https://x.test/p?a=1&b=2");
    }

    #[test]
    fn disabled_params_are_dropped() {
        let params = vec![Kv {
            key: "off".to_string(),
            value: "1".to_string(),
            enabled: false,
            description: String::new(),
        }];
        assert_eq!(model::compose_url("https://x.test/p", &params), "https://x.test/p");
    }

    #[test]
    fn pulls_query_out_of_url() {
        let mut url = "https://x.test/p?a=1&b=2".to_string();
        let mut params = vec![Kv::new("a", "dup")];
        pull_query_into_params(&mut url, &mut params);
        assert_eq!(url, "https://x.test/p");
        assert_eq!(params.len(), 2);
        assert_eq!(params[1].value, "2");
    }

    #[test]
    fn base64_matches_standard_encoding() {
        assert_eq!(send::base64(b""), "");
        assert_eq!(send::base64(b"u"), "dQ==");
        assert_eq!(send::base64(b"user:pass"), "dXNlcjpwYXNz");
    }

    #[test]
    fn postman_roundtrip_preserves_request() {
        let mut col = Collection::new("Round trip");
        col.add_request("<root>", sample());
        col.folders.push(collections::Folder {
            name: "Inner".to_string(),
            requests: vec![RequestDef {
                name: "Create".to_string(),
                method: "POST".to_string(),
                url: "https://api.test/v1/things".to_string(),
                body_mode: BodyMode::Raw,
                body: "{\"a\":1}".to_string(),
                ..Default::default()
            }],
        });

        let json = collections::to_postman_json(&col);
        assert!(json["info"]["schema"]
            .as_str()
            .unwrap()
            .contains("v2.1.0"));
        let back = collections::from_postman_json(&json);
        assert_eq!(back.name, "Round trip");
        assert_eq!(back.total_requests(), 2);
        let got = &back.requests[0];
        assert_eq!(got.method, "GET");
        assert_eq!(got.url, "https://api.test/v1/things");
        assert_eq!(got.headers[0].value, "{{trace_id}}");
        assert!(matches!(&got.auth, model::Auth::Bearer { token } if token == "{{token}}"));
        assert_eq!(back.folders[0].name, "Inner");
        assert_eq!(back.folders[0].requests[0].body, "{\"a\":1}");
    }

    #[test]
    fn curl_bash_includes_auth_and_params() {
        let snippet = curl::curl_bash(&sample(), &vars());
        assert!(snippet.contains("Authorization"));
        assert!(snippet.contains("Bearer secret123"));
        assert!(snippet.contains("limit=20"));
        assert!(snippet.contains("X-Trace"));
    }

    #[test]
    fn python_snippet_uses_method() {
        let snippet = curl::python(&sample(), &vars());
        assert!(snippet.starts_with("import requests"));
        assert!(snippet.contains("requests.get(url"));
    }

    #[test]
    fn unknown_variable_is_reported() {
        let mut ws = Workspace::default();
        ws.active_environment = None;
        ws.collections = vec![];
        assert_eq!(ws.unresolved("{{missing}}"), vec!["missing".to_string()]);
        assert!(ws.unresolved("plain").is_empty());
    }

    #[test]
    fn method_bodyless_rules() {
        let mut req = sample();
        assert!(req.is_bodyless());
        req.set_method(1);
        assert_eq!(req.method, "POST");
        assert!(!req.is_bodyless());
    }

    #[test]
    fn workspace_vars_prefer_environment_over_collection() {
        let mut ws = Workspace::default();
        ws.active_environment = Some("prod".to_string());
        ws.environments.push(Environment {
            name: "prod".to_string(),
            variables: vec![Kv::new("base_url", "https://prod.test")],
        });
        let vars = ws.vars();
        assert_eq!(vars.get("base_url").map(String::as_str), Some("https://prod.test"));
    }
}

#[cfg(test)]
mod ui_tests {
    use super::*;

    fn raw_input() -> egui::RawInput {
        let mut raw = egui::RawInput::default();
        raw.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1400.0, 900.0),
        ));
        raw.time = Some(0.0);
        raw
    }

    fn frame(app: &mut App, ctx: &Context) {
        let _ = ctx.run(raw_input(), |ctx| app.update_ui(ctx));
    }

    fn request() -> RequestDef {
        RequestDef {
            name: "List things".to_string(),
            method: "GET".to_string(),
            url: "https://httpbin.org/get".to_string(),
            params: vec![Kv::new("limit", "20")],
            headers: vec![Kv::new("Accept", "application/json")],
            auth: model::Auth::Bearer {
                token: "{{token}}".to_string(),
            },
            ..Default::default()
        }
    }

    fn response(status: u16) -> ResponseBundle {
        ResponseBundle {
            status,
            status_text: "OK".to_string(),
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            cookies: vec![("session".to_string(), "abc".to_string())],
            body: "{\"slideshow\": {\"title\": \"Sample\"}}".to_string(),
            body_bytes: 34,
            content_type: Some("application/json".to_string()),
            elapsed_ms: 42,
            final_url: "https://httpbin.org/get".to_string(),
            error: None,
        }
    }

    fn app_with_response() -> (App, Context) {
        let mut app = App::with_workspace(Workspace::default());
        app.tabs[0].resp = Some(response(200));
        (app, Context::default())
    }

    #[test]
    fn renders_default_state_twice() {
        let (mut app, ctx) = (App::with_workspace(Workspace::default()), Context::default());
        frame(&mut app, &ctx);
        frame(&mut app, &ctx);
    }

    #[test]
    fn renders_every_request_tab() {
        let (mut app, ctx) = (App::with_workspace(Workspace::default()), Context::default());
        for view in [
            ReqView::Params,
            ReqView::Headers,
            ReqView::Body,
            ReqView::Auth,
            ReqView::Settings,
        ] {
            app.tabs[0].req_view = view;
            app.tabs[0].req.set_method(1);
            frame(&mut app, &ctx);
        }
    }

    #[test]
    fn renders_every_response_view() {
        let (mut app, ctx) = app_with_response();
        for view in [
            RespView::Pretty,
            RespView::Raw,
            RespView::Headers,
            RespView::Cookies,
            RespView::Code,
            RespView::Meta,
        ] {
            app.tabs[0].resp_view = view;
            frame(&mut app, &ctx);
        }
    }

    #[test]
    fn renders_code_snippets_for_each_language() {
        let (mut app, ctx) = app_with_response();
        app.tabs[0].resp_view = RespView::Code;
        for lang in 0..CODE_LANGS.len() {
            app.tabs[0].code_lang = lang;
            frame(&mut app, &ctx);
        }
    }

    #[test]
    fn renders_failed_request() {
        let (mut app, ctx) = (App::with_workspace(Workspace::default()), Context::default());
        app.tabs[0].resp = Some(ResponseBundle::failure(
            "could not connect: dns failure".to_string(),
            12,
        ));
        frame(&mut app, &ctx);
        app.tabs[0].resp_view = RespView::Meta;
        frame(&mut app, &ctx);
    }

    #[test]
    fn renders_each_sidebar_with_content() {
        let (mut app, ctx) = app_with_response();
        app.ws.history.push(HistoryRecord {
            at: "2026-10-28 12:00:00".to_string(),
            name: "GET echo".to_string(),
            method: "GET".to_string(),
            url: "https://httpbin.org/get".to_string(),
            status: Some(200),
            elapsed_ms: 42,
            body_bytes: 34,
            request: app.tabs[0].req.clone(),
        });
        app.ws.environments.push(Environment {
            name: "stage".to_string(),
            variables: vec![Kv::new("base_url", "https://stage.test")],
        });
        app.ws.active_environment = Some("stage".to_string());
        for side in [Sidebar::Collections, Sidebar::Environments, Sidebar::History] {
            app.sidebar = side;
            frame(&mut app, &ctx);
        }
    }

    #[test]
    fn renders_every_dialog() {
        let (mut app, ctx) = app_with_response();
        let dialogs = vec![
            Dialog::Save {
                name: "Sample".to_string(),
                collection: 0,
                folder: "<root>".to_string(),
            },
            Dialog::Io {
                mode: IoMode::Import,
                path: String::new(),
                content: "{}".to_string(),
                collection: 0,
            },
            Dialog::Io {
                mode: IoMode::Export,
                path: "out.json".to_string(),
                content: String::new(),
                collection: 0,
            },
            Dialog::Confirm {
                title: "Confirm".to_string(),
                message: "test".to_string(),
                pending: Pending::ClearHistory,
            },
            Dialog::Settings,
            Dialog::About,
        ];
        for dialog in dialogs {
            app.dialog = Some(dialog);
            frame(&mut app, &ctx);
            assert!(app.dialog.is_some(), "dialog should stay open until dismissed");
        }
    }

    #[test]
    fn tab_open_and_close_round_trip() {
        let mut app = App::with_workspace(Workspace::default());
        let before = app.tabs.len();
        app.open_tab(request());
        assert_eq!(app.tabs.len(), before + 1);
        assert_eq!(app.active, app.tabs.len() - 1);
        app.close_tab(app.active);
        assert_eq!(app.tabs.len(), before);
    }

    #[test]
    fn sending_flag_clears_when_response_arrives() {
        let mut app = App::with_workspace(Workspace::default());
        app.ws.history.clear();
        let id = app.tabs[0].id;
        app.tabs[0].sending = true;
        let _ = app.tx;
        drop(app.rx.try_recv());
        app.tx.send((id, response(201))).unwrap();
        app.drain_responses();
        assert!(!app.tabs[0].sending);
        assert_eq!(app.tabs[0].resp.as_ref().unwrap().status, 201);
        assert_eq!(app.ws.history.len(), 1);
    }

    #[test]
    fn save_request_lands_in_collection() {
        let mut app = App::with_workspace(Workspace::default());
        app.tabs[0].req.url = "https://httpbin.org/get".to_string();
        app.save_request_to(0, "<root>", "Saved thing");
        let col = &app.ws.collections[0];
        assert!(col.requests.iter().any(|r| r.name == "Saved thing"));
        assert_eq!(app.tabs[0].req.name, "Saved thing");
    }
}

#[cfg(test)]
mod theme_tests {
    use super::*;
    use egui::Color32;

    fn render(app: &mut App, ctx: &Context) {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 900.0),
            )),
            time: Some(0.0),
            ..Default::default()
        };
        let _ = ctx.run(raw, |ctx| app.update_ui(ctx));
    }

    /// WCAG relative luminance.
    fn luminance(c: Color32) -> f32 {
        let lin = |v: f32| {
            if v <= 0.03924 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c.r() as f32 / 255.0)
            + 0.7152 * lin(c.g() as f32 / 255.0)
            + 0.0722 * lin(c.b() as f32 / 255.0)
    }

    fn contrast(a: Color32, b: Color32) -> f32 {
        let (l1, l2) = (luminance(a), luminance(b));
        let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
        (hi + 0.05) / (lo + 0.05)
    }

    #[test]
    fn both_palettes_are_legible_on_their_own_backgrounds() {
        for dark in [true, false] {
            let palette = Palette::for_dark(dark);
            let visuals = theme::visuals(dark, &palette);
            let panel = visuals.panel_fill;
            let window_fill = visuals.window_fill;
            let field = visuals.extreme_bg_color;
            let text = visuals.override_text_color.unwrap();
            for (name, color) in [
                ("ok", palette.ok),
                ("warn", palette.warn),
                ("err", palette.err),
                ("info", palette.info),
                ("muted", palette.muted),
                ("get", palette.get),
                ("delete", palette.delete),
            ] {
                assert!(
                    contrast(color, panel) >= 4.5 && contrast(color, window_fill) >= 4.5
                        && contrast(color, field) >= 4.5,
                    "{name} on dark={dark} has contrast {:.2}/{:.2}/{:.2}, needs 4.5",
                    contrast(color, panel),
                    contrast(color, window_fill),
                    contrast(color, field)
                );
            }
            assert!(
                contrast(text, panel) >= 4.5 && contrast(text, window_fill) >= 4.5,
                "default body text unreadable on dark={dark}"
            );
            assert!(
                contrast(palette.muted, panel) >= 4.5,
                "secondary text unreadable on dark={dark}"
            );
        }
    }

    #[test]
    fn settings_round_trip_and_old_files_still_load() {
        let written = serde_json::to_string(&Workspace::default()).unwrap();
        let back: Workspace = serde_json::from_str(&written).unwrap();
        assert_eq!(back.settings.theme, Theme::System);
        assert!(written.contains("\"theme\""), "theme must be persisted");

        // A workspace.json from the previous build had "dark_theme" and no "theme".
        let legacy = r#"{"settings":{"proxy_url":"","default_timeout_secs":30,"dark_theme":true,"pretty_by_default":true}}"#;
        let parsed: Workspace = serde_json::from_str(legacy).unwrap();
        assert_eq!(parsed.settings.theme, Theme::System);
        assert_eq!(parsed.settings.default_timeout_secs, 30);
    }

    #[test]
    fn system_theme_follows_the_os() {
        assert!(Theme::resolve(Theme::System, Some(egui::Theme::Dark)));
        assert!(!Theme::resolve(Theme::System, Some(egui::Theme::Light)));
        assert!(!Theme::resolve(Theme::System, None));
        assert!(Theme::resolve(Theme::Dark, Some(egui::Theme::Light)));
        assert!(!Theme::resolve(Theme::Light, Some(egui::Theme::Dark)));
    }

    #[test]
    fn palettes_differ_between_themes() {
        assert_ne!(
            Palette::dark().muted,
            Palette::light().muted
        );
    }

    #[test]
    fn starts_with_an_empty_request() {
        let app = App::with_workspace(Workspace::default());
        assert_eq!(app.tabs.len(), 1);
        assert!(app.tabs[0].req.url.is_empty());
        assert!(app.tabs[0].resp.is_none());
        assert!(!app.tabs[0].sending);
    }

    #[test]
    fn blank_variable_names_resolve_nothing_and_named_ones_do() {
        let mut ws = Workspace::default();
        ws.active_environment = Some("qa".to_string());
        ws.environments.push(Environment {
            name: "qa".to_string(),
            variables: vec![Kv::new("", "oops"), Kv::new("host", "https://qa.test")],
        });
        let vars = ws.vars();
        assert!(!vars.contains_key(""), "an empty key must not become a variable");
        assert_eq!(vars.get("host").map(String::as_str), Some("https://qa.test"));
    }

    #[test]
    fn environment_rows_render_with_blank_keys() {
        let (mut app, ctx) = (App::with_workspace(Workspace::default()), Context::default());
        app.ws.environments.push(Environment {
            name: String::new(),
            variables: vec![Kv::new("", ""), Kv::new("token", "abc")],
        });
        app.ws.active_environment = None;
        app.sidebar = Sidebar::Environments;
        render(&mut app, &ctx);
        // Every environment shows its variables, not only the active one.
        let inactive = app
            .ws
            .environments
            .iter()
            .filter(|e| Some(&e.name) != app.ws.active_environment.as_ref())
            .count();
        assert!(inactive >= 1);
    }

    #[test]
    fn light_theme_frame_renders() {
        let (mut app, ctx) = (App::with_workspace(Workspace::default()), Context::default());
        app.ws.settings.theme = Theme::Light;
        app.palette = apply(&ctx, Theme::Light);
        render(&mut app, &ctx);
        assert_eq!(app.ws.settings.theme, Theme::Light);
    }
}
