//! Euppa i Ruppa: allt i Entra ID som kort, knappar och formulär.
//!
//! Ruppa är bara ett skal. Områden, åtgärder, fält och uppslag läses ur den installerade euppa.py
//! (AREAS, ACTIONS, LOOKUPS via core::describe), och allt körs som euppa-kommandon:
//! `euppa kör ÅTGÄRD Namn=värde … --json [--ja]`, `euppa uppslag TYP sök --json`, `euppa tenant NAMN`,
//! `euppa logga-in`, `euppa logga-ut` och `euppa kom-ihåg på|av`. Nya åtgärder i Euppa syns här direkt.

use crate::core;
use crate::gui::xterm;
use crate::pages::{Action, Job, open_path};
use crate::theme::{self, GREEN, MUTED as DIM, RED, TEXT, YELLOW};
use eframe::egui::{self, Color32, RichText, Ui, vec2};
use std::collections::HashMap;
use std::path::PathBuf;

const ACCENT: u8 = 205;

#[derive(Clone, Debug, PartialEq)]
pub struct EParam {
    pub name: String,
    pub question: String,
    pub default: String,
    pub optional: bool,
    /// uppslag i tenanten, t.ex. "user+" (+ = flera värden)
    pub lookup: String,
    /// the values Euppa is sent (Swedish) and the labels shown (in the chosen language)
    pub choices: Vec<String>,
    pub choice_labels: Vec<String>,
}

impl EParam {
    /// The label shown for a choice value.
    fn choice_label<'a>(&'a self, v: &'a str) -> &'a str {
        self.choices
            .iter()
            .position(|c| c == v)
            .and_then(|i| self.choice_labels.get(i))
            .map(|s| s.as_str())
            .unwrap_or(v)
    }
}

impl EParam {
    fn kind(&self) -> &str {
        self.lookup.trim_end_matches('+')
    }
    fn many(&self) -> bool {
        self.lookup.ends_with('+')
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EAction {
    pub id: String,
    pub name: String,
    pub area: String,
    pub kind: String,
    pub scopes: Vec<String>,
    pub desc: String,
    pub params: Vec<EParam>,
}

impl EAction {
    pub fn writes(&self) -> bool {
        self.kind.starts_with("änd")
    }
    /// Fältet som tar användare (för användarflödet): ett obligatoriskt @user-fält.
    pub fn user_param(&self) -> Option<&EParam> {
        self.params.iter().find(|p| p.kind() == "user" && !p.optional)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EArea {
    /// the key (Swedish, as the actions refer to it) and the name shown
    pub name: String,
    pub label: String,
    pub icon: String,
    pub color: u8,
    pub desc: String,
}

/// En rad ur ett uppslag: värde (det som skickas), namn och detalj.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub value: String,
    pub name: String,
    pub detail: String,
}

fn s(v: &serde_json::Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

/// The English field when English is chosen and Euppa has it, otherwise the Swedish one.
fn loc(x: &serde_json::Value, en: &str, sv: &str) -> String {
    let e = s(&x[en]);
    if crate::core::is_sv() || e.is_empty() { s(&x[sv]) } else { e }
}

/// AREAS, ACTIONS och LOOKUPS (som core::describe ger dem) → områden, åtgärder och uppslagens namn.
pub fn parse_schema(v: &serde_json::Value) -> (Vec<EArea>, Vec<EAction>, HashMap<String, String>) {
    let areas = v["AREAS"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|x| EArea {
                    name: s(&x["namn"]),
                    label: loc(x, "name", "namn"),
                    icon: s(&x["ikon"]),
                    color: x["farg"].as_u64().unwrap_or(ACCENT as u64) as u8,
                    desc: loc(x, "desc", "beskrivning"),
                })
                .collect()
        })
        .unwrap_or_default();
    let actions = v["ACTIONS"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|x| EAction {
                    id: s(&x["id"]),
                    name: loc(x, "name", "namn"),
                    area: s(&x["omrade"]),
                    kind: s(&x["typ"]),
                    scopes: x["scopes"].as_array().map(|l| l.iter().map(s).collect()).unwrap_or_default(),
                    desc: loc(x, "desc", "beskrivning"),
                    params: x["params"]
                        .as_array()
                        .map(|l| {
                            l.iter()
                                .map(|p| EParam {
                                    name: s(&p["namn"]),
                                    question: loc(p, "question", "fraga"),
                                    default: s(&p["standard"]),
                                    optional: p["valfri"].as_bool().unwrap_or(false),
                                    lookup: s(&p["uppslag"]),
                                    choices: p["val"].as_array().map(|c| c.iter().map(s).collect()).unwrap_or_default(),
                                    choice_labels: if crate::core::is_sv() { &p["val"] } else { &p["choices"] }
                                        .as_array()
                                        .or(p["val"].as_array())
                                        .map(|c| c.iter().map(s).collect())
                                        .unwrap_or_default(),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .filter(|a| !a.id.is_empty())
                .collect()
        })
        .unwrap_or_default();
    // LOOKUPS = {typ: [scopes, "etikett", kod]}
    let lookups =
        v["LOOKUPS"].as_object().map(|o| o.iter().map(|(k, x)| (k.clone(), s(&x[1]))).collect()).unwrap_or_default();
    (areas, actions, lookups)
}

/// Sista JSON-raden i ett jobbs utdata (Euppa skriver en enda rad JSON vid --json).
pub fn last_json(lines: &[String]) -> Option<serde_json::Value> {
    lines.iter().rev().filter(|l| l.trim_start().starts_with('{')).find_map(|l| serde_json::from_str(l.trim()).ok())
}

pub fn cell(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(x) => x.clone(),
        serde_json::Value::Array(a) => a.iter().map(cell).collect::<Vec<_>>().join(", "),
        x => x.to_string(),
    }
}

/// JSON-raderna från `euppa kör … --json` → kolumner (i första radens ordning) och celler.
pub fn parse_rows(v: &serde_json::Value) -> (Vec<String>, Vec<Vec<String>>) {
    let rows = v["rows"].as_array().cloned().unwrap_or_default();
    let mut cols: Vec<String> = Vec::new();
    for r in &rows {
        if let Some(o) = r.as_object() {
            for k in o.keys() {
                if !cols.contains(k) {
                    cols.push(k.clone());
                }
            }
        }
    }
    let data = rows.iter().map(|r| cols.iter().map(|c| cell(&r[c])).collect()).collect();
    (cols, data)
}

pub fn parse_items(v: &serde_json::Value) -> Vec<Item> {
    v["items"]
        .as_array()
        .map(|a| {
            a.iter().map(|x| Item { value: s(&x["value"]), name: s(&x["name"]), detail: s(&x["detail"]) }).collect()
        })
        .unwrap_or_default()
}

/// CSV med semikolon och BOM, som Euppa själv skriver (öppnas rätt i svensk Excel).
pub fn to_csv(cols: &[String], rows: &[Vec<String>]) -> String {
    let q = |x: &str| {
        if x.contains([';', '"', '\n', '\r']) { format!("\"{}\"", x.replace('"', "\"\"")) } else { x.to_string() }
    };
    let mut out = String::from("\u{feff}");
    out += &cols.iter().map(|c| q(c)).collect::<Vec<_>>().join(";");
    out += "\r\n";
    for r in rows {
        out += &r.iter().map(|c| q(c)).collect::<Vec<_>>().join(";");
        out += "\r\n";
    }
    out
}

fn matches(q: &str, text: &str) -> bool {
    let t = text.to_lowercase();
    q.to_lowercase().split_whitespace().all(|w| t.contains(w))
}

#[derive(Default, Clone, Copy, PartialEq)]
enum View {
    #[default]
    Users,
    Areas,
}

/// Vad bakgrundsjobbet för data gör: hämtar användare, eller söker till ett fält.
enum Fetch {
    Users(Job),
    Lookup(String, Job),
}

struct Res {
    action: String,
    ok: bool,
    error: String,
    cols: Vec<String>,
    rows: Vec<Vec<String>>,
}

#[derive(Default)]
pub struct EuppaPage {
    loaded: bool,
    areas: Vec<EArea>,
    actions: Vec<EAction>,
    lookups: HashMap<String, String>,
    tenants: Vec<String>,
    active: String,
    keep: bool,
    view: View,
    search: String,
    area: Option<String>,
    sel: Option<String>,
    values: HashMap<String, String>,
    confirm: bool,
    job: Option<Job>,
    job_open: bool,
    check: bool,
    result: Option<Res>,
    saved: Option<PathBuf>,
    fetch: Option<Fetch>,
    users: Vec<Item>,
    users_tenant: String,
    user_filter: String,
    picked: Vec<String>,
    /// träffar för ett fält: (fält, träffar)
    hits: Option<(String, Vec<Item>)>,
    fetch_error: String,
}

fn euppa_dir() -> PathBuf {
    std::env::var_os("EUPPA_DIR")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| core::home().join(".euppa"))
}

impl EuppaPage {
    fn load(&mut self) {
        if let Some(v) = core::describe("euppa", &["AREAS", "ACTIONS", "LOOKUPS"]) {
            let (a, b, c) = parse_schema(&v);
            self.areas = a;
            self.actions = b;
            self.lookups = c;
        }
        self.load_tenants();
        // för skärmdumpar och tester: RUPPA_EUPPA_OPEN="Område/åtgärd-id" (tomt område = användarflödet)
        if !self.loaded
            && let Ok(v) = std::env::var("RUPPA_EUPPA_OPEN")
        {
            let (a, id) = v.split_once('/').unwrap_or((&v, ""));
            if !a.is_empty() {
                self.view = View::Areas;
                self.area = Some(a.to_string());
            }
            if let Some(x) = self.actions.iter().find(|x| x.id == id).cloned() {
                self.pick(&x);
            }
        }
        self.loaded = true;
    }

    fn load_tenants(&mut self) {
        let dir = euppa_dir();
        let read = |f: &str| -> serde_json::Value {
            std::fs::read_to_string(dir.join(f)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
        };
        let k = read("kunder.json");
        self.tenants = k["tenants"]
            .as_array()
            .map(|a| a.iter().filter_map(|t| t["name"].as_str().map(String::from)).collect())
            .unwrap_or_default();
        let cfg = read("config.json");
        self.active = cfg["active"].as_str().unwrap_or("").to_string();
        // samma standard som Euppa: av på macOS (nyckelringsfrågor), på annars
        self.keep = cfg["keep_logins"].as_bool().unwrap_or(!cfg!(target_os = "macos"));
    }

    /// Läs om åtgärder och tenants (efter en uppdatering av Euppa).
    pub fn reset(&mut self) {
        self.loaded = false;
    }

    /// An area's name in the chosen language.
    fn area_label(&self, name: &str) -> String {
        self.areas.iter().find(|a| a.name == name).map(|a| a.label.clone()).unwrap_or_else(|| name.to_string())
    }

    fn area(&self, name: &str) -> (Color32, String, String) {
        match self.areas.iter().find(|a| a.name == name) {
            Some(a) => (xterm(a.color), a.icon.clone(), a.desc.clone()),
            None => (xterm(ACCENT), name.chars().take(2).collect::<String>().to_uppercase(), String::new()),
        }
    }

    fn pick(&mut self, a: &EAction) {
        if self.sel.as_deref() != Some(&a.id) {
            self.sel = Some(a.id.clone());
            // standardvärden med platshållare ({domain} …) fyller Euppa själv i
            self.values = a
                .params
                .iter()
                .map(|p| (p.name.clone(), if p.default.contains('{') { String::new() } else { p.default.clone() }))
                .collect();
            self.confirm = false;
            self.hits = None;
        }
    }

    fn start(&mut self, ctx: &egui::Context, args: Vec<String>, title: String) -> Option<Action> {
        match Job::start("euppa", &args, title, ctx) {
            Ok(j) => {
                self.job = Some(j);
                self.job_open = false;
                self.check = true;
                None
            }
            Err(e) => Some(Action::Toast(e)),
        }
    }

    fn fetch_users(&mut self, ctx: &egui::Context) -> Option<Action> {
        let args = vec!["uppslag".into(), "user".into(), "--alla".into(), "--json".into()];
        match Job::start("euppa", &args, tr!("Fetching the users", "Hämtar användarna"), ctx) {
            Ok(j) => {
                self.fetch = Some(Fetch::Users(j));
                self.fetch_error.clear();
                None
            }
            Err(e) => Some(Action::Toast(e)),
        }
    }

    fn lookup(&mut self, ctx: &egui::Context, p: &EParam, q: &str) -> Option<Action> {
        let mut args = vec!["uppslag".to_string(), p.kind().to_string()];
        if !q.trim().is_empty() {
            args.push(q.trim().to_string());
        }
        args.push("--json".into());
        match Job::start("euppa", &args, tr!(format!("Searching {}", p.kind()), format!("Söker {}", p.kind())), ctx) {
            Ok(j) => {
                self.fetch = Some(Fetch::Lookup(p.name.clone(), j));
                self.fetch_error.clear();
                None
            }
            Err(e) => Some(Action::Toast(e)),
        }
    }

    fn args_for(&self, a: &EAction, json: bool) -> Vec<String> {
        let mut v = vec!["kör".to_string(), a.id.clone()];
        for p in &a.params {
            let val = if self.view == View::Users && Some(p) == a.user_param() && !self.picked.is_empty() {
                self.picked.join(",")
            } else {
                self.values.get(&p.name).cloned().unwrap_or_default()
            };
            if !val.trim().is_empty() {
                v.push(format!("{}={}", p.name, val.trim()));
            }
        }
        if json {
            v.push("--json".into());
        }
        if a.writes() && self.confirm {
            v.push("--ja".into());
        }
        v
    }

    /// Ta hand om jobb som blivit klara.
    fn poll(&mut self) {
        if self.check
            && let Some(ok) = self.job.as_ref().and_then(|j| j.finished())
        {
            self.check = false;
            self.load_tenants(); // en inloggning kan ha lagt till en tenant
            let text = self.job.as_ref().map(|j| j.text()).unwrap_or_default();
            if let Some(v) = last_json(&text)
                && v.get("rows").is_some()
            {
                let (cols, rows) = parse_rows(&v);
                self.result = Some(Res {
                    action: s(&v["action"]),
                    ok: ok && v["ok"].as_bool().unwrap_or(false),
                    error: s(&v["error"]),
                    cols,
                    rows,
                });
                self.saved = None;
                if !ok {
                    self.job_open = true;
                }
            }
        }
        let done = match &self.fetch {
            Some(Fetch::Users(j)) | Some(Fetch::Lookup(_, j)) => j.finished().is_some(),
            None => false,
        };
        if done && let Some(f) = self.fetch.take() {
            let (j, field) = match f {
                Fetch::Users(j) => (j, None),
                Fetch::Lookup(n, j) => (j, Some(n)),
            };
            let text = j.text();
            match last_json(&text) {
                Some(v) if v["ok"].as_bool() == Some(true) => {
                    let items = parse_items(&v);
                    match field {
                        None => {
                            self.users = items;
                            self.users_tenant = s(&v["tenant"]);
                            let have: Vec<String> = self.users.iter().map(|u| u.value.clone()).collect();
                            self.picked.retain(|p| have.contains(p));
                        }
                        Some(n) => self.hits = Some((n, items)),
                    }
                    self.load_tenants();
                }
                other => {
                    self.fetch_error = other
                        .map(|v| s(&v["error"]))
                        .filter(|e| !e.is_empty())
                        .or_else(|| text.iter().rev().find(|l| l.contains('✘')).cloned())
                        .unwrap_or_else(|| {
                            tr!(
                                "That didn't work. Are you signed in? (Sign in at the top right.)",
                                "Det gick inte. Är du inloggad? (Logga in uppe till höger.)"
                            )
                            .into()
                        });
                }
            }
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, ctx: &egui::Context) -> Option<Action> {
        if !self.loaded {
            self.load();
        }
        self.poll();
        let mut act = None;
        let busy = self.job.as_ref().is_some_and(|j| j.finished().is_none());
        let fetching = self.fetch.is_some();
        let accent = xterm(ACCENT);

        if self.actions.is_empty() {
            theme::card(ui, |ui| {
                ui.label(RichText::new(tr!("Found no actions in Euppa.", "Hittar inga åtgärder i Euppa.")).size(16.0));
                ui.label(
                    RichText::new(tr!(
                        "Update Euppa at the top right, or open it in the terminal.",
                        "Uppdatera Euppa uppe till höger, eller öppna den i terminalen."
                    ))
                    .color(DIM),
                );
                if theme::secondary(ui, true, "Terminal").clicked() {
                    act = Some(Action::Terminal(vec![]));
                }
            });
            return act;
        }

        // verktygsrad: tenant · vy · sök · inloggning
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("TENANT").color(DIM).size(11.0).extra_letter_spacing(0.8));
                    let shown = if self.active.is_empty() {
                        tr!("None selected", "Ingen vald").to_string()
                    } else {
                        self.active.clone()
                    };
                    let mut pick = None;
                    egui::ComboBox::from_id_salt("euppa-tenant")
                        .selected_text(RichText::new(shown).size(15.0))
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for t in &self.tenants {
                                if ui.selectable_label(*t == self.active, t).clicked() {
                                    pick = Some(t.clone());
                                }
                            }
                            if self.tenants.is_empty() {
                                ui.label(
                                    RichText::new(tr!(
                                        "The list fills up when you sign in.",
                                        "Listan fylls på när du loggar in."
                                    ))
                                    .color(DIM),
                                );
                            }
                        });
                    if let Some(t) = pick {
                        act = self.start(
                            ctx,
                            vec!["tenant".into(), t.clone()],
                            tr!(format!("Switching to {t}"), format!("Växlar till {t}")),
                        );
                        self.active = t;
                        self.users.clear();
                    }
                });
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(tr!("VIEW", "VISA")).color(DIM).size(11.0).extra_letter_spacing(0.8));
                    ui.horizontal(|ui| {
                        let mut u = self.view == View::Users;
                        if theme::chip(ui, &mut u, tr!("Users", "Användare"), accent) {
                            self.view = View::Users;
                            self.sel = None;
                        }
                        let mut a = self.view == View::Areas;
                        if theme::chip(ui, &mut a, tr!("Areas", "Områden"), accent) {
                            self.view = View::Areas;
                            self.sel = None;
                        }
                    });
                });
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(tr!("SEARCH", "SÖK")).color(DIM).size(11.0).extra_letter_spacing(0.8));
                    let w = (ui.available_width() - 330.0).max(160.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text(tr!(
                                format!("Search {} actions…", self.actions.len()),
                                format!("Sök bland {} åtgärder…", self.actions.len())
                            ))
                            .desired_width(w)
                            .margin(vec2(10.0, 8.0)),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                    if theme::secondary(ui, true, "Terminal")
                        .on_hover_text(tr!("Euppa in the built-in terminal", "Euppa i den inbyggda terminalen"))
                        .clicked()
                    {
                        act = Some(Action::Terminal(vec![]));
                    }
                    if theme::secondary(ui, !busy, tr!("Sign out", "Logga ut"))
                        .on_hover_text("Disconnect-MgGraph")
                        .clicked()
                    {
                        act = self.start(ctx, vec!["logga-ut".into()], tr!("Signing out", "Loggar ut").into());
                        self.users.clear();
                    }
                    if theme::primary(ui, !busy, tr!("Sign in", "Logga in"), accent.gamma_multiply(0.85)) {
                        act = self.start(
                            ctx,
                            vec!["logga-in".into()],
                            tr!("Signing in to Microsoft Graph", "Loggar in i Microsoft Graph").into(),
                        );
                    }
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let mut keep = self.keep;
                if theme::toggle(
                    ui,
                    &mut keep,
                    tr!("Remember the sign-in between runs", "Kom ihåg inloggningen mellan körningar"),
                ) {
                    self.keep = keep;
                    let v = if keep { "på" } else { "av" };
                    act = self.start(
                        ctx,
                        vec!["kom-ihåg".into(), v.into()],
                        tr!(format!("Remember sign-ins: {v}"), format!("Kom ihåg inloggningar: {v}")),
                    );
                }
                ui.label(
                    RichText::new(if self.keep {
                        tr!(
                            "The sign-in is kept in the keychain, you sign in once.",
                            "Inloggningen sparas i nyckelringen, du loggar in en gång."
                        )
                    } else {
                        tr!(
                            "Off: every run signs in again (no keychain prompt on macOS).",
                            "Av: varje körning loggar in på nytt (ingen nyckelringsfråga på macOS)."
                        )
                    })
                    .color(DIM)
                    .size(12.0),
                );
            });
        });
        ui.add_space(14.0);

        let selected = self.sel.as_ref().and_then(|id| self.actions.iter().find(|a| &a.id == id)).cloned();
        let avail = ui.available_width();
        let side = selected.is_some() && avail > 820.0;
        let detail_w = if side { (avail * 0.4).clamp(340.0, 460.0) } else { avail };
        let main_w = if side { avail - detail_w - 16.0 } else { avail };

        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(main_w);
                egui::ScrollArea::vertical().id_salt("euppa-main").auto_shrink([false, false]).show(ui, |ui| {
                    if !self.search.trim().is_empty() {
                        let q = self.search.trim().to_string();
                        let hits: Vec<EAction> = self
                            .actions
                            .iter()
                            .filter(|a| {
                                matches(
                                    &q,
                                    &format!("{} {} {} {} {}", a.name, a.area, self.area_label(&a.area), a.desc, a.id),
                                )
                            })
                            .cloned()
                            .collect();
                        theme::overline(ui, &tr!(format!("{} matches", hits.len()), format!("{} träffar", hits.len())));
                        if hits.is_empty() {
                            ui.label(
                                RichText::new(tr!(
                                    "No actions match. Try another word.",
                                    "Inga åtgärder matchar. Prova ett annat ord."
                                ))
                                .color(DIM),
                            );
                        }
                        self.action_grid(ui, &hits);
                    } else if self.view == View::Users {
                        if let Some(a) = self.users_view(ui, ctx, fetching) {
                            act = Some(a);
                        }
                    } else {
                        self.areas_view(ui);
                    }
                });
            });
            if let Some(a) = selected.clone().filter(|_| side) {
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    ui.set_width(detail_w);
                    egui::ScrollArea::vertical().id_salt("euppa-detail").auto_shrink([false, false]).show(ui, |ui| {
                        act = self.detail(ui, ctx, &a, busy, fetching).or(act.take());
                    });
                });
            }
        });
        if let Some(a) = selected.filter(|_| !side) {
            ui.add_space(10.0);
            act = self.detail(ui, ctx, &a, busy, fetching).or(act);
        }
        act
    }

    fn badges(a: &EAction) -> Vec<(String, Color32)> {
        let mut b = vec![if a.writes() {
            (tr!("changes", "ändrar").to_string(), theme::WRITE)
        } else if a.kind.starts_with("lok") {
            ("lokal".to_string(), DIM)
        } else {
            (tr!("reads", "läser").to_string(), GREEN)
        }];
        let n = a.params.iter().filter(|p| a.user_param() != Some(p)).count();
        if n > 0 {
            b.push((tr!(format!("{n} field{}", if n == 1 { "" } else { "s" }), format!("{n} fält")), DIM));
        }
        b
    }

    fn action_grid(&mut self, ui: &mut Ui, list: &[EAction]) {
        let sel = self.sel.clone();
        let areas = self.areas.clone();
        let mut clicked: Option<EAction> = None;
        theme::grid(ui, list, 240.0, 140.0, |ui, a, size| {
            let (col, icon) = areas
                .iter()
                .find(|x| x.name == a.area)
                .map(|x| (xterm(x.color), x.icon.clone()))
                .unwrap_or((xterm(ACCENT), "E".into()));
            let on = sel.as_deref() == Some(&a.id);
            if theme::tile(ui, size, col, &icon, &a.name, &a.desc, &Self::badges(a), on).clicked() {
                clicked = Some(a.clone());
            }
        });
        if let Some(a) = clicked {
            self.pick(&a);
        }
    }

    fn areas_view(&mut self, ui: &mut Ui) {
        if let Some(name) = self.area.clone() {
            let (col, icon, hint) = self.area(&name);
            if theme::ghost(ui, tr!("← All areas", "← Alla områden")) {
                self.area = None;
                return;
            }
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(44.0, 44.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 12.0, col.gamma_multiply(0.2));
                ui.painter().text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    &icon,
                    egui::FontId::proportional(16.0),
                    col,
                );
                ui.vertical(|ui| {
                    ui.label(RichText::new(self.area_label(&name)).size(22.0).color(TEXT));
                    ui.label(RichText::new(hint).color(DIM));
                });
            });
            ui.add_space(10.0);
            let list: Vec<EAction> = self.actions.iter().filter(|a| a.area == name).cloned().collect();
            let (reads, writes): (Vec<EAction>, Vec<EAction>) = list.into_iter().partition(|a| !a.writes());
            if !reads.is_empty() {
                theme::overline(
                    ui,
                    &tr!(format!("View and export · {}", reads.len()), format!("Visa och exportera · {}", reads.len())),
                );
                self.action_grid(ui, &reads);
            }
            if !writes.is_empty() {
                theme::overline(ui, &tr!(format!("Change · {}", writes.len()), format!("Ändra · {}", writes.len())));
                self.action_grid(ui, &writes);
            }
            return;
        }
        theme::overline(ui, tr!("Areas", "Områden"));
        let cats: Vec<(EArea, usize, usize)> = self
            .areas
            .iter()
            .map(|a| {
                let n = self.actions.iter().filter(|x| x.area == a.name).count();
                let w = self.actions.iter().filter(|x| x.area == a.name && x.writes()).count();
                (a.clone(), n, w)
            })
            .filter(|(_, n, _)| *n > 0)
            .collect();
        let mut open = None;
        theme::grid(ui, &cats, 230.0, 146.0, |ui, (a, n, w), size| {
            let mut b = vec![(tr!(format!("{n} actions"), format!("{n} åtgärder")), xterm(a.color))];
            if *w > 0 {
                b.push((tr!(format!("{w} change"), format!("{w} ändrar")), theme::WRITE));
            }
            if theme::tile(ui, size, xterm(a.color), &a.icon, &a.label, &a.desc, &b, false).clicked() {
                open = Some(a.name.clone());
            }
        });
        if let Some(a) = open {
            self.area = Some(a);
        }
    }

    /// Användarflödet: hämta tenantens användare, kryssa en eller flera, välj vad som ska hända.
    fn users_view(&mut self, ui: &mut Ui, ctx: &egui::Context, fetching: bool) -> Option<Action> {
        let mut act = None;
        let accent = xterm(ACCENT);
        let fetching_users = matches!(self.fetch, Some(Fetch::Users(_)));
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                let title = if self.users.is_empty() {
                    tr!("Users", "Användare").to_string()
                } else {
                    tr!(
                        format!("Users in {} · {}", self.users_tenant, self.users.len()),
                        format!("Användare i {} · {}", self.users_tenant, self.users.len())
                    )
                };
                ui.label(RichText::new(title).size(16.0).color(TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if fetching_users {
                        ui.add(egui::Spinner::new().color(accent));
                        ui.label(
                            RichText::new(tr!("Fetching … (signs in if needed)", "Hämtar … (loggar in vid behov)"))
                                .color(DIM),
                        );
                    } else {
                        let label = if self.users.is_empty() {
                            tr!("Fetch users", "Hämta användare")
                        } else {
                            tr!("Fetch again", "Hämta igen")
                        };
                        if theme::primary(ui, !fetching, label, accent.gamma_multiply(0.85)) {
                            act = self.fetch_users(ctx);
                        }
                    }
                });
            });
            if !self.fetch_error.is_empty() && !matches!(self.fetch, Some(Fetch::Lookup(..))) && self.hits.is_none() {
                ui.label(RichText::new(&self.fetch_error).color(RED));
            }
            if self.users.is_empty() {
                ui.label(
                    RichText::new(tr!(
                        "Fetch the users in the tenant, tick one or more and choose what should happen to them: \
                             add to a group, assign a license, block, reset MFA …",
                        "Hämta användarna i tenanten, kryssa en eller flera och välj vad som ska hända med dem: \
                             lägga till i grupp, tilldela licens, spärra, återställa MFA …"
                    ))
                    .color(DIM),
                );
                return;
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.user_filter)
                        .hint_text(tr!("Filter on name, email, title …", "Filtrera på namn, e-post, titel …"))
                        .desired_width((ui.available_width() - 280.0).max(160.0))
                        .margin(vec2(10.0, 8.0)),
                );
                let f = self.user_filter.clone();
                let vis: Vec<String> = self
                    .users
                    .iter()
                    .filter(|u| matches(&f, &format!("{} {} {}", u.name, u.value, u.detail)))
                    .map(|u| u.value.clone())
                    .collect();
                if theme::secondary(
                    ui,
                    !vis.is_empty(),
                    &tr!(format!("Tick all ({})", vis.len()), format!("Kryssa alla ({})", vis.len())),
                )
                .clicked()
                {
                    for v in vis {
                        if !self.picked.contains(&v) {
                            self.picked.push(v);
                        }
                    }
                }
                if theme::ghost(ui, "Rensa") {
                    self.picked.clear();
                }
            });
            ui.add_space(4.0);
            let f = self.user_filter.clone();
            let vis: Vec<Item> = self
                .users
                .iter()
                .filter(|u| matches(&f, &format!("{} {} {}", u.name, u.value, u.detail)))
                .cloned()
                .collect();
            egui::ScrollArea::vertical().id_salt("euppa-users").max_height(300.0).auto_shrink([false, true]).show_rows(
                ui,
                24.0,
                vis.len(),
                |ui, range| {
                    for u in &vis[range] {
                        ui.horizontal(|ui| {
                            let mut on = self.picked.contains(&u.value);
                            if ui.checkbox(&mut on, RichText::new(&u.name).color(TEXT)).changed() {
                                if on {
                                    self.picked.push(u.value.clone());
                                } else {
                                    self.picked.retain(|p| p != &u.value);
                                }
                            }
                            ui.label(RichText::new(&u.detail).color(DIM).size(12.5));
                        });
                    }
                },
            );
        });
        ui.add_space(12.0);
        if self.picked.is_empty() {
            return act;
        }
        let names: Vec<String> = self
            .picked
            .iter()
            .take(3)
            .map(|v| self.users.iter().find(|u| &u.value == v).map(|u| u.name.clone()).unwrap_or(v.clone()))
            .collect();
        let more = if self.picked.len() > 3 {
            tr!(format!(" and {} more", self.picked.len() - 3), format!(" och {} till", self.picked.len() - 3))
        } else {
            String::new()
        };
        theme::overline(
            ui,
            &tr!(
                format!("What do you want to do with {}{more}?", names.join(", ")),
                format!("Vad vill du göra med {}{more}?", names.join(", "))
            ),
        );
        let order: Vec<String> = self.areas.iter().map(|a| a.name.clone()).collect();
        let mut list: Vec<EAction> = self.actions.iter().filter(|a| a.user_param().is_some()).cloned().collect();
        list.sort_by_key(|a| (a.writes(), order.iter().position(|o| o == &a.area).unwrap_or(99)));
        let (reads, writes): (Vec<EAction>, Vec<EAction>) = list.into_iter().partition(|a| !a.writes());
        theme::overline(ui, &tr!(format!("View · {}", reads.len()), format!("Visa · {}", reads.len())));
        self.action_grid(ui, &reads);
        theme::overline(ui, &tr!(format!("Change · {}", writes.len()), format!("Ändra · {}", writes.len())));
        self.action_grid(ui, &writes);
        act
    }

    fn detail(&mut self, ui: &mut Ui, ctx: &egui::Context, a: &EAction, busy: bool, fetching: bool) -> Option<Action> {
        let mut act = None;
        let (col, icon, _) = self.area(&a.area);
        let accent = xterm(ACCENT);
        let user_p = a.user_param().cloned();
        let users_mode = self.view == View::Users && !self.picked.is_empty() && self.search.trim().is_empty();
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                let (rr, _) = ui.allocate_exact_size(vec2(38.0, 38.0), egui::Sense::hover());
                ui.painter().rect_filled(rr, 10.0, col.gamma_multiply(0.2));
                ui.painter().text(
                    rr.center(),
                    egui::Align2::CENTER_CENTER,
                    &icon,
                    egui::FontId::proportional(14.0),
                    col,
                );
                ui.label(RichText::new(self.area_label(&a.area)).color(DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::ghost(ui, tr!("Close", "Stäng")) {
                        self.sel = None;
                    }
                });
            });
            ui.add_space(6.0);
            ui.label(RichText::new(&a.name).size(21.0).color(TEXT));
            ui.horizontal_wrapped(|ui| {
                let (t, c) = if a.writes() {
                    (tr!("changes the tenant", "ändrar i tenanten"), theme::WRITE)
                } else {
                    (tr!("only reads", "läser bara"), GREEN)
                };
                theme::badge(ui, t, c);
                if !a.scopes.is_empty() {
                    theme::badge(ui, &a.scopes.join(" · "), DIM);
                }
            });
            if !a.desc.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(&a.desc).color(TEXT.gamma_multiply(0.9)));
            }
            if !a.params.is_empty() {
                ui.add_space(10.0);
            }
            for p in &a.params {
                let label = format!("{}{}", p.question, if p.optional { " (valfritt)" } else { "" });
                ui.label(RichText::new(label).color(DIM).size(12.5));
                if users_mode && user_p.as_ref() == Some(p) {
                    ui.horizontal_wrapped(|ui| {
                        theme::badge(
                            ui,
                            &tr!(
                                format!("{} selected users", self.picked.len()),
                                format!("{} valda användare", self.picked.len())
                            ),
                            accent,
                        );
                    });
                    ui.add_space(4.0);
                    continue;
                }
                let v = self.values.entry(p.name.clone()).or_default();
                if !p.choices.is_empty() {
                    if v.is_empty() {
                        *v = p.choices[0].clone();
                    }
                    if p.choices.len() <= 4 {
                        ui.horizontal_wrapped(|ui| {
                            for ch in &p.choices {
                                if ui.selectable_label(v == ch, p.choice_label(ch)).clicked() {
                                    *v = ch.clone();
                                }
                            }
                        });
                    } else {
                        egui::ComboBox::from_id_salt(format!("euppa-{}-{}", a.id, p.name))
                            .selected_text(p.choice_label(v).to_string())
                            .width(ui.available_width() - 8.0)
                            .show_ui(ui, |ui| {
                                for ch in &p.choices {
                                    if ui.selectable_label(v == ch, p.choice_label(ch)).clicked() {
                                        *v = ch.clone();
                                    }
                                }
                            });
                    }
                } else if !p.lookup.is_empty() {
                    let mut search = false;
                    ui.horizontal(|ui| {
                        let hint = match p.kind() {
                            "user" => tr!("name or email…", "namn eller e-post…"),
                            _ => tr!("part of the name…", "en del av namnet…"),
                        };
                        let r = ui.add(
                            egui::TextEdit::singleline(v)
                                .hint_text(hint)
                                .desired_width(ui.available_width() - 70.0)
                                .margin(vec2(10.0, 8.0)),
                        );
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            search = true;
                        }
                        if theme::secondary(ui, !fetching, tr!("Search", "Sök")).clicked() {
                            search = true;
                        }
                    });
                    if search {
                        // med flera värden söker vi på det som står efter sista kommat
                        let q = v.rsplit(',').next().unwrap_or("").trim().to_string();
                        act = self.lookup(ctx, p, &q);
                    }
                    if let Some(Fetch::Lookup(n, _)) = &self.fetch
                        && n == &p.name
                    {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new().color(accent));
                            ui.label(
                                RichText::new(tr!(
                                    "Searching the tenant … (signs in if needed)",
                                    "Söker i tenanten … (loggar in vid behov)"
                                ))
                                .color(DIM),
                            );
                        });
                    }
                    let mut chosen = None;
                    if let Some((n, items)) = &self.hits
                        && n == &p.name
                    {
                        let label = self.lookups.get(p.kind()).cloned().unwrap_or_default();
                        egui::Frame::new().fill(theme::SURFACE2).corner_radius(10).inner_margin(8).show(ui, |ui| {
                            ui.label(RichText::new(format!("{} {label}", items.len())).color(DIM).size(12.0));
                            egui::ScrollArea::vertical().id_salt(format!("hits-{n}")).max_height(180.0).show(
                                ui,
                                |ui| {
                                    for it in items {
                                        let r = ui.selectable_label(
                                            false,
                                            RichText::new(format!("{}   {}", it.name, it.detail)).size(13.0),
                                        );
                                        if r.clicked() {
                                            chosen = Some(it.value.clone());
                                        }
                                    }
                                },
                            );
                        });
                    }
                    if let Some(c) = chosen {
                        let v = self.values.entry(p.name.clone()).or_default();
                        if p.many() && v.contains(',') {
                            let mut parts: Vec<String> = v.split(',').map(|x| x.trim().to_string()).collect();
                            parts.pop();
                            parts.push(c);
                            *v = parts.join(",");
                        } else if p.many()
                            && !v.trim().is_empty()
                            && self.hits.as_ref().is_some_and(|h| h.1.iter().any(|i| i.value == v.trim()))
                        {
                            *v = format!("{},{c}", v.trim());
                        } else {
                            *v = c;
                        }
                        self.hits = None;
                    }
                } else {
                    let hint = if p.default.contains('{') {
                        tr!("Euppa fills it in (the tenant's domain)", "Euppa fyller i (tenantens domän)")
                    } else {
                        ""
                    };
                    ui.add(
                        egui::TextEdit::singleline(v)
                            .hint_text(hint)
                            .desired_width(f32::INFINITY)
                            .margin(vec2(10.0, 8.0)),
                    );
                }
                ui.add_space(4.0);
            }
            if !self.fetch_error.is_empty() && self.hits.is_none() && !matches!(self.fetch, Some(Fetch::Users(_))) {
                ui.label(RichText::new(&self.fetch_error).color(RED).size(12.5));
            }
            let missing: Vec<String> = a
                .params
                .iter()
                .filter(|p| !p.optional && p.default.is_empty())
                .filter(|p| !(users_mode && user_p.as_ref() == Some(p)))
                .filter(|p| self.values.get(&p.name).is_none_or(|v| v.trim().is_empty()))
                .map(|p| p.question.clone())
                .collect();
            if a.writes() {
                ui.add_space(6.0);
                theme::toggle(
                    ui,
                    &mut self.confirm,
                    tr!("I know this changes the tenant", "Jag vet att det här ändrar i tenanten"),
                );
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let can = !busy && missing.is_empty() && (!a.writes() || self.confirm);
                let label = if users_mode && user_p.is_some() {
                    tr!(
                        format!("  Run for {} users  ", self.picked.len()),
                        format!("  Kör för {} användare  ", self.picked.len())
                    )
                } else {
                    tr!("  Run  ", "  Kör  ").to_string()
                };
                if theme::primary(ui, can, &label, accent.gamma_multiply(0.85)) {
                    let args = self.args_for(a, true);
                    act = self.start(ctx, args, tr!(format!("Run {}", a.name), format!("Kör {}", a.name)));
                    self.result = None;
                }
                if theme::secondary(ui, true, tr!("Run in terminal", "Kör i terminal"))
                    .on_hover_text(tr!(
                        "with questions and search in Euppa's own menu",
                        "med frågor och sökning i Euppas egen meny"
                    ))
                    .clicked()
                {
                    act = Some(Action::Terminal(self.args_for(a, false)));
                }
            });
            if !missing.is_empty() {
                ui.label(
                    RichText::new(tr!(
                        format!("Fill in: {}", missing.join(", ")),
                        format!("Fyll i: {}", missing.join(", "))
                    ))
                    .color(YELLOW)
                    .size(12.5),
                );
            }
        });
        ui.add_space(10.0);
        self.job_panel(ui);
        self.result_card(ui, a);
        act
    }

    /// Senaste jobbet: status, sista raden och vid behov hela loggen.
    fn job_panel(&mut self, ui: &mut Ui) {
        let Some(job) = &self.job else { return };
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                match job.finished() {
                    None => {
                        ui.add(egui::Spinner::new().color(YELLOW));
                    }
                    Some(true) => theme::badge(ui, "klart", GREEN),
                    Some(false) => theme::badge(ui, "misslyckades", RED),
                }
                ui.label(RichText::new(&job.title).color(TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::ghost(
                        ui,
                        if self.job_open {
                            tr!("Hide details", "Dölj detaljer")
                        } else {
                            tr!("Show details", "Visa detaljer")
                        },
                    ) {
                        self.job_open = !self.job_open;
                    }
                });
            });
            let text = job.text();
            if !self.job_open {
                if let Some(l) = text.iter().rev().find(|l| {
                    let t = l.trim();
                    !t.is_empty() && !t.starts_with('$') && !t.starts_with('{') && !t.starts_with(['│', '┌', '└'])
                }) {
                    ui.label(RichText::new(crate::term::cut(l.trim(), 140)).color(DIM).size(12.5));
                }
                return;
            }
            ui.add_space(6.0);
            egui::Frame::new().fill(Color32::from_rgb(0x07, 0x09, 0x0d)).corner_radius(10).inner_margin(12).show(
                ui,
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("euppa-log")
                        .stick_to_bottom(true)
                        .max_height(260.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for l in text.iter().filter(|l| !l.trim_start().starts_with('{')) {
                                let c = if l.contains('✘') {
                                    RED
                                } else if l.contains('✔') {
                                    GREEN
                                } else if l.trim_start().starts_with('!') || l.starts_with('$') {
                                    DIM
                                } else {
                                    TEXT.gamma_multiply(0.9)
                                };
                                ui.label(RichText::new(l).monospace().size(12.0).color(c));
                            }
                        });
                },
            );
        });
        ui.add_space(8.0);
    }

    fn result_card(&mut self, ui: &mut Ui, a: &EAction) {
        let Some(res) = &self.result else { return };
        if res.action != a.id {
            return;
        }
        let mut save = false;
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                let head = if res.rows.is_empty() {
                    if res.ok {
                        tr!("Done – nothing to show", "Klart – inget att visa").to_string()
                    } else {
                        tr!("It didn't work", "Det gick inte").to_string()
                    }
                } else {
                    format!("Resultat · {} rader", res.rows.len())
                };
                ui.label(RichText::new(head).size(16.0).color(if res.ok { TEXT } else { RED }));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !res.rows.is_empty() {
                        if theme::secondary(ui, true, tr!("Save as CSV", "Spara som CSV")).clicked() {
                            save = true;
                        }
                        if let Some(p) = &self.saved
                            && theme::ghost(ui, tr!("Open", "Öppna"))
                        {
                            open_path(p);
                        }
                    }
                });
            });
            if !res.error.is_empty() {
                ui.label(RichText::new(&res.error).color(RED));
            }
            if !res.ok && res.error.is_empty() {
                ui.label(
                    RichText::new(tr!("The error is in the details above.", "Felet står i detaljerna ovanför."))
                        .color(DIM),
                );
            }
            if let Some(p) = &self.saved {
                ui.label(RichText::new(format!("Sparat i {}", p.display())).color(GREEN).size(12.5));
            }
            if res.rows.is_empty() {
                return;
            }
            ui.add_space(6.0);
            egui::ScrollArea::both().id_salt("euppa-result").max_height(380.0).show(ui, |ui| {
                egui::Grid::new("euppa-rows").striped(true).spacing(vec2(18.0, 6.0)).show(ui, |ui| {
                    for c in &res.cols {
                        ui.label(RichText::new(c).color(DIM).size(12.5));
                    }
                    ui.end_row();
                    for row in res.rows.iter().take(2000) {
                        for c in row {
                            let bad = c.starts_with("Fel:");
                            ui.label(RichText::new(crate::term::cut(c, 90)).color(if bad { RED } else { TEXT }))
                                .on_hover_text(c);
                        }
                        ui.end_row();
                    }
                });
            });
        });
        if save && let Some(res) = &self.result {
            let dir = core::home().join("Documents").join("Euppa");
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or_default();
            let tenant =
                if self.active.is_empty() { "euppa".to_string() } else { self.active.replace([' ', '/'], "-") };
            let p = dir.join(format!("{tenant}-{}-{stamp}.csv", res.action));
            match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&p, to_csv(&res.cols, &res.rows))) {
                Ok(()) => self.saved = Some(p),
                Err(e) => {
                    self.saved =
                        Some(PathBuf::from(tr!(format!("(could not save: {e})"), format!("(kunde inte spara: {e})"))))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn schema_and_rows() {
        let v = json!({
            "AREAS": [{"namn": "Användare", "ikon": "AN", "farg": 49, "beskrivning": "x"}],
            "ACTIONS": [{"id": "anvandare-sparra", "namn": "Spärra", "omrade": "Användare", "typ": "ändrar",
                         "scopes": ["User.ReadWrite.All"], "beskrivning": "d",
                         "params": [{"namn": "Anvandare", "fraga": "Vilka?", "standard": "", "valfri": false,
                                     "uppslag": "user+", "val": []},
                                    {"namn": "Byt", "fraga": "Byt?", "standard": "Ja", "valfri": false,
                                     "uppslag": "", "val": ["Ja", "Nej"]}]}],
            "LOOKUPS": {"user": [["User.Read.All"], "användare", "kod"]}
        });
        let (areas, actions, lookups) = parse_schema(&v);
        assert_eq!(areas[0].color, 49);
        assert!(actions[0].writes());
        assert_eq!(actions[0].user_param().map(|p| p.name.as_str()), Some("Anvandare"));
        assert!(actions[0].params[0].many());
        assert_eq!(actions[0].params[1].choices, vec!["Ja", "Nej"]);
        assert_eq!(lookups["user"], "användare");

        let lines = vec![
            "  E› Klart".to_string(),
            r#"{"ok": true, "action": "x", "rows": [{"A": 1, "B": ["a","b"]}, {"A": null, "C": "z"}]}"#.to_string(),
        ];
        let j = last_json(&lines).unwrap();
        let (cols, rows) = parse_rows(&j);
        assert_eq!(cols, vec!["A", "B", "C"]);
        assert_eq!(rows[0], vec!["1", "a, b", ""]);
        assert_eq!(rows[1], vec!["", "", "z"]);
        assert_eq!(to_csv(&cols, &rows[..1]), "\u{feff}A;B;C\r\n1;a, b;\r\n");
        assert!(matches("anna it", "Anna Svensson anna@x.se IT"));
        assert!(!matches("bo", "Anna"));
    }

    /// Den riktiga euppa.py (bredvid ruppa i Dev Projects) ska gå att läsa som schema.
    #[test]
    fn schema_from_python() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../euppa/euppa.py");
        let Some(py) = core::find_python() else { return };
        if !path.exists() {
            return;
        }
        let code = "import importlib.util,json,sys\nspec=importlib.util.spec_from_file_location('k',sys.argv[1])\n\
                    m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)\n\
                    print(json.dumps({k:getattr(m,k) for k in ('AREAS','ACTIONS','LOOKUPS')},ensure_ascii=False))";
        let out = std::process::Command::new(&py[0]).args(&py[1..]).arg("-c").arg(code).arg(&path).output().unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let (areas, actions, lookups) = parse_schema(&v);
        assert!(areas.len() >= 8);
        assert!(actions.len() >= 50);
        for a in &actions {
            assert!(areas.iter().any(|x| x.name == a.area), "{} saknar område", a.id);
            for p in &a.params {
                if !p.lookup.is_empty() {
                    assert!(lookups.contains_key(p.kind()), "{}: okänt uppslag {}", a.id, p.lookup);
                }
            }
        }
        assert!(actions.iter().filter(|a| a.user_param().is_some()).count() >= 15);
    }
}
