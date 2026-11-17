//! Kusinernas egna sidor i Ruppa: knappar, listor och formulär i stället för en terminal.
//! Under huven kör Ruppa kusinernas icke-interaktiva kommandon (uppa status/update, puppa kör, muppa skapa)
//! och läser deras data direkt (Uppas cache, Puppas recept och tenants). Terminalen finns kvar som "Avancerat".

use crate::core;
use crate::gui::xterm;
use crate::theme::{self, GREEN, MUTED as DIM, RED, TEXT, YELLOW};
use eframe::egui::{self, Color32, RichText, Ui, vec2};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

/// Vad en sida vill att fönstret ska göra.
pub enum Action {
    /// öppna kusinen i den inbyggda terminalen, med de här argumenten
    Terminal(Vec<String>),
    Toast(String),
}

// ───────────────────────────── jobb ─────────────────────────────

/// Ett kusin-kommando som körs i bakgrunden. Utdatan samlas rad för rad.
pub struct Job {
    pub title: String,
    pub lines: Arc<Mutex<Vec<String>>>,
    pub done: Arc<Mutex<Option<bool>>>,
}

impl Job {
    pub fn start(name: &str, args: &[String], title: impl Into<String>, ctx: &egui::Context) -> Result<Job, String> {
        let script = core::cousin_dir(name).join(format!("{name}.py"));
        if !script.exists() {
            return Err(format!("{name} är inte installerad"));
        }
        let py = core::find_python().ok_or("hittar ingen Python 3.8+")?;
        let mut cmd = Command::new(&py[0]);
        cmd.args(&py[1..])
            .arg(&script)
            .args(args)
            .env("NO_COLOR", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONUTF8", "1")
            .env("UPPA_NO_ANIM", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd.spawn().map_err(|e| format!("kunde inte starta {name}: {e}"))?;
        let lines = Arc::new(Mutex::new(vec![format!("$ {name} {}", args.join(" "))]));
        let done = Arc::new(Mutex::new(None));
        let mut readers = Vec::new();
        let outs: Vec<Box<dyn Read + Send>> =
            vec![Box::new(child.stdout.take().expect("stdout")), Box::new(child.stderr.take().expect("stderr"))];
        for r in outs {
            let (l2, c2) = (lines.clone(), ctx.clone());
            readers.push(thread::spawn(move || {
                let mut br = BufReader::new(r);
                let mut buf = Vec::new();
                while let Ok(n) = br.read_until(b'\n', &mut buf) {
                    if n == 0 {
                        break;
                    }
                    let s = String::from_utf8_lossy(&buf);
                    let s = core::strip_ansi(s.trim_end_matches(['\n', '\r']));
                    let s = s.rsplit('\r').next().unwrap_or("").to_string();
                    if let Ok(mut l) = l2.lock() {
                        l.push(s);
                    }
                    c2.request_repaint();
                    buf.clear();
                }
            }));
        }
        let (d2, c2) = (done.clone(), ctx.clone());
        thread::spawn(move || {
            for r in readers {
                let _ = r.join();
            }
            let ok = child.wait().map(|s| s.success()).unwrap_or(false);
            if let Ok(mut d) = d2.lock() {
                *d = Some(ok);
            }
            c2.request_repaint();
        });
        Ok(Job { title: title.into(), lines, done })
    }

    pub fn finished(&self) -> Option<bool> {
        self.done.lock().ok().and_then(|d| *d)
    }

    pub fn text(&self) -> Vec<String> {
        self.lines.lock().map(|l| l.clone()).unwrap_or_default()
    }
}

/// Utdata från senaste jobbet: en statusrad, och vid behov hela loggen.
fn job_panel(ui: &mut Ui, job: &Option<Job>, open: &mut bool) {
    let Some(job) = job else { return };
    ui.add_space(4.0);
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
                if theme::ghost(ui, if *open { "Dölj detaljer" } else { "Visa detaljer" }) {
                    *open = !*open;
                }
            });
        });
        if !*open {
            if let Some(last) = job.text().iter().rev().find(|l| !l.trim().is_empty() && !l.starts_with('$')) {
                ui.label(RichText::new(crate::term::cut(last, 140)).color(DIM).size(12.5));
            }
            return;
        }
        ui.add_space(6.0);
        egui::Frame::new().fill(Color32::from_rgb(0x07, 0x09, 0x0d)).corner_radius(10).inner_margin(12).show(
            ui,
            |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(&job.title)
                    .stick_to_bottom(true)
                    .max_height(240.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for l in job.text() {
                            let col = if l.contains('✘') || l.to_lowercase().contains("error") || l.contains("misslyck")
                            {
                                RED
                            } else if l.contains('✔') {
                                GREEN
                            } else if l.starts_with('$') {
                                DIM
                            } else {
                                TEXT.gamma_multiply(0.9)
                            };
                            ui.label(RichText::new(l).monospace().size(12.0).color(col));
                        }
                    });
            },
        );
    });
    ui.add_space(8.0);
}

fn section(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    theme::card_titled(ui, title, "", add);
}

fn section_sub(ui: &mut Ui, title: &str, sub: &str, add: impl FnOnce(&mut Ui)) {
    theme::card_titled(ui, title, sub, add);
}

fn field(ui: &mut Ui, text: &mut String, hint: &str, width: f32) {
    ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(width).margin(vec2(10.0, 8.0)));
}

pub fn open_path(p: &Path) {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = Command::new(cmd).arg(p).spawn();
}

// ───────────────────────────── Uppa ─────────────────────────────

#[derive(Clone, Debug)]
pub struct UItem {
    pub source: String,
    pub name: String,
    pub label: String,
    pub current: String,
    pub latest: String,
    pub manual: bool,
    pub note: String,
}

impl UItem {
    pub fn outdated(&self) -> bool {
        core::newer(&self.latest, &self.current)
    }
}

/// Läs Uppas cache (~/.uppa/cache.json): {"sources": {"brew": {"items": [...], "summary": "...", "error": ""}}, "time": …}
pub fn parse_uppa_cache(text: &str) -> (Vec<UItem>, BTreeMap<String, (String, String)>, f64) {
    let v: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
    let mut items = Vec::new();
    let mut sources = BTreeMap::new();
    if let Some(src) = v["sources"].as_object() {
        for (k, s) in src {
            sources.insert(
                k.clone(),
                (core::strip_ansi(s["summary"].as_str().unwrap_or("")), s["error"].as_str().unwrap_or("").to_string()),
            );
            for it in s["items"].as_array().cloned().unwrap_or_default() {
                let g = |f: &str| it[f].as_str().unwrap_or("").to_string();
                items.push(UItem {
                    source: k.clone(),
                    name: g("name"),
                    label: if g("label").is_empty() { g("name") } else { g("label") },
                    current: g("current"),
                    latest: g("latest"),
                    manual: it["manual"].as_bool().unwrap_or(false),
                    note: core::strip_ansi(&g("note")),
                });
            }
        }
    }
    (items, sources, v["time"].as_f64().unwrap_or(0.0))
}

const SOURCE_TITLES: [(&str, &str); 9] = [
    ("self", "Uppa"),
    ("brew", "Homebrew"),
    ("winget", "winget"),
    ("pwsh", "PowerShell-moduler"),
    ("az", "az-tillägg"),
    ("npm", "npm (Microsoft-paket)"),
    ("dotnet", ".NET-verktyg"),
    ("puppa", "Puppa"),
    ("muppa", "Muppa"),
];

fn source_title(k: &str) -> String {
    SOURCE_TITLES.iter().find(|(a, _)| *a == k).map(|(_, b)| b.to_string()).unwrap_or_else(|| k.to_string())
}

#[derive(Default)]
pub struct UppaPage {
    items: Vec<UItem>,
    sources: BTreeMap<String, (String, String)>,
    time: f64,
    loaded: bool,
    selected: HashSet<(String, String)>,
    show_all: bool,
    job: Option<Job>,
    job_open: bool,
    reload_after: bool,
}

impl UppaPage {
    fn cache_path() -> PathBuf {
        core::home().join(".uppa").join("cache.json")
    }

    fn load(&mut self) {
        let text = std::fs::read_to_string(Self::cache_path()).unwrap_or_default();
        let (items, sources, time) = parse_uppa_cache(&text);
        self.items = items;
        self.sources = sources;
        self.time = time;
        self.selected = self
            .items
            .iter()
            .filter(|i| i.outdated() && !i.manual)
            .map(|i| (i.source.clone(), i.name.clone()))
            .collect();
        self.loaded = true;
    }

    fn run(&mut self, ctx: &egui::Context, args: &[&str], title: &str) -> Option<Action> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        match Job::start("uppa", &args, title, ctx) {
            Ok(j) => {
                self.job = Some(j);
                self.job_open = false;
                self.reload_after = true;
                None
            }
            Err(e) => Some(Action::Toast(e)),
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, ctx: &egui::Context) -> Option<Action> {
        let mut act = None;
        if !self.loaded {
            self.load();
            let age = now_secs() - self.time;
            if self.items.is_empty() || age > 6.0 * 3600.0 {
                act = self.run(ctx, &["status", "--refresh", "--quiet"], "Kollar efter uppdateringar");
            }
        }
        if self.reload_after && self.job.as_ref().and_then(|j| j.finished()).is_some() {
            self.reload_after = false;
            self.load();
        }
        let busy = self.job.as_ref().is_some_and(|j| j.finished().is_none());
        let n_outdated = self.items.iter().filter(|i| i.outdated()).count();
        let chosen: Vec<String> = self
            .items
            .iter()
            .filter(|i| self.selected.contains(&(i.source.clone(), i.name.clone())))
            .map(|i| i.name.clone())
            .collect();
        let accent = xterm(36);

        // sammanfattning + huvudknappar
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    let (big, col) = if self.items.is_empty() {
                        ("Inte skannat än".to_string(), DIM)
                    } else if n_outdated == 0 {
                        ("Allt är uppdaterat".to_string(), GREEN)
                    } else {
                        (format!("{n_outdated} kan uppdateras"), YELLOW)
                    };
                    ui.label(RichText::new(big).size(22.0).color(col));
                    let sub = if self.time > 0.0 {
                        format!(
                            "{} verktyg och moduler · senast kollat {}",
                            self.items.len(),
                            ago(now_secs() - self.time)
                        )
                    } else {
                        "Uppa kollar Homebrew/winget, PowerShell-moduler, az, npm och .NET".into()
                    };
                    ui.label(RichText::new(sub).color(DIM).size(12.5));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if chosen.is_empty() {
                        "Uppdatera valda".to_string()
                    } else {
                        format!("Uppdatera valda ({})", chosen.len())
                    };
                    if theme::primary(ui, !busy && !chosen.is_empty(), &label, accent) {
                        let mut a: Vec<&str> = vec!["update", "-y"];
                        a.extend(chosen.iter().map(|s| s.as_str()));
                        act = self.run(ctx, &a, &format!("Uppdaterar {} st", chosen.len()));
                    }
                    if theme::secondary(ui, !busy, "Kolla igen").clicked() {
                        act = self.run(ctx, &["status", "--refresh", "--quiet"], "Kollar efter uppdateringar");
                    }
                });
            });
        });
        ui.add_space(8.0);
        job_panel(ui, &self.job, &mut self.job_open);

        egui::ScrollArea::vertical().id_salt("uppa-lista").auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                theme::overline(ui, "Det du har installerat");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    theme::toggle(ui, &mut self.show_all, "Visa även det som är aktuellt");
                });
            });
            if self.items.is_empty() && busy {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label(
                        RichText::new("Uppa skannar dina verktyg. Det tar en halv minut första gången.").color(DIM),
                    );
                });
            }
            let mut by_src: BTreeMap<usize, (String, Vec<UItem>)> = BTreeMap::new();
            for it in &self.items {
                if !self.show_all && !it.outdated() {
                    continue;
                }
                let order = SOURCE_TITLES.iter().position(|(a, _)| *a == it.source).unwrap_or(99);
                by_src.entry(order).or_insert_with(|| (it.source.clone(), Vec::new())).1.push(it.clone());
            }
            if by_src.is_empty() && !self.items.is_empty() {
                theme::card(ui, |ui| {
                    ui.label(RichText::new("✔  Inget att uppdatera just nu.").color(GREEN).size(15.0));
                });
                ui.add_space(12.0);
            }
            for (_, (src, its)) in by_src {
                let (summary, err) = self.sources.get(&src).cloned().unwrap_or_default();
                section_sub(ui, &source_title(&src), &summary, |ui| {
                    if !err.is_empty() {
                        ui.label(RichText::new(err).color(RED));
                    }
                    egui::Grid::new(format!("g-{src}")).num_columns(4).spacing(vec2(16.0, 8.0)).striped(true).show(
                        ui,
                        |ui| {
                            for it in its {
                                let key = (it.source.clone(), it.name.clone());
                                if it.outdated() && !it.manual {
                                    let mut on = self.selected.contains(&key);
                                    if ui.checkbox(&mut on, "").changed() {
                                        if on {
                                            self.selected.insert(key);
                                        } else {
                                            self.selected.remove(&key);
                                        }
                                    }
                                } else {
                                    ui.label("");
                                }
                                ui.label(RichText::new(&it.label).color(TEXT));
                                if it.outdated() {
                                    ui.label(
                                        RichText::new(format!("{}  →  {}", it.current, it.latest))
                                            .monospace()
                                            .color(YELLOW),
                                    );
                                } else {
                                    ui.label(RichText::new(&it.current).monospace().color(DIM));
                                }
                                let note = if it.manual {
                                    format!("uppdateras för hand. {}", it.note)
                                } else {
                                    it.note.clone()
                                };
                                ui.label(RichText::new(note).color(DIM).size(12.0));
                                ui.end_row();
                            }
                        },
                    );
                });
            }
            ui.columns(2, |c| {
                section_sub(&mut c[0], "Grundpaket", "Installera det som behövs för ett visst jobb", |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (k, label) in [("bas", "Bas"), ("m365", "M365"), ("azure", "Azure"), ("alla", "Alla")] {
                            if theme::secondary(ui, !busy, label).clicked() {
                                act =
                                    self.run(ctx, &["install", "-y", k], &format!("Installerar grundpaketet {label}"));
                            }
                        }
                    });
                });
                section_sub(&mut c[1], "Verktyg", "Utloggning, hälsokoll och städning", |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if theme::secondary(ui, !busy, "Logga ut från allt")
                            .on_hover_text("Graph, Exchange, PnP, Az, az, m365 …")
                            .clicked()
                        {
                            act = self.run(ctx, &["logout", "-y"], "Loggar ut från allt");
                        }
                        if theme::secondary(ui, !busy, "Hälsokoll").clicked() {
                            act = self.run(ctx, &["doctor"], "Hälsokoll");
                        }
                        if theme::secondary(ui, !busy, "Städa gamla versioner").clicked() {
                            act = self.run(ctx, &["clean", "-y"], "Städar gamla versioner");
                        }
                        if theme::secondary(ui, true, "Öppna i terminal").clicked() {
                            act = Some(Action::Terminal(vec![]));
                        }
                    });
                });
            });
        });
        act
    }
}

fn now_secs() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn ago(s: f64) -> String {
    let s = s.max(0.0) as u64;
    match s {
        0..=59 => "nyss".into(),
        60..=3599 => format!("för {} min sedan", s / 60),
        3600..=86_399 => format!("för {} h sedan", s / 3600),
        _ => format!("för {} dagar sedan", s / 86_400),
    }
}

// ───────────────────────────── Puppa ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct RParam {
    pub name: String,
    pub label: String,
    pub default: String,
    pub lookup: String,
}

#[derive(Clone, Debug)]
pub struct Recipe {
    pub id: String,
    pub name: String,
    pub category: String,
    pub kind: String,
    pub needs: String,
    pub desc: String,
    pub params: Vec<RParam>,
}

impl Recipe {
    pub fn writes(&self) -> bool {
        let k = self.kind.to_lowercase();
        k.starts_with("änd") || k.starts_with("and") || k == "skriv"
    }
}

/// Läs huvudet i ett Puppa-recept: <#PUPPA … #> med "nyckel: värde"-rader och "param: Namn | Etikett | standard | @uppslag".
pub fn parse_recipe(id: &str, text: &str) -> Option<Recipe> {
    let start = text.find("<#PUPPA")?;
    let end = text[start..].find("#>")? + start;
    let mut meta: HashMap<String, String> = HashMap::new();
    let mut params = Vec::new();
    for line in text[start + 7..end].lines() {
        let Some((k, v)) = line.split_once(':') else { continue };
        let k = k.trim().to_lowercase().replace('ä', "a");
        let v = v.trim();
        if k == "param" {
            let parts: Vec<&str> = v.split('|').map(|s| s.trim()).collect();
            params.push(RParam {
                name: parts.first().copied().unwrap_or("").into(),
                label: parts.get(1).copied().filter(|s| !s.is_empty()).unwrap_or(parts[0]).into(),
                default: parts.get(2).copied().unwrap_or("").into(),
                lookup: parts.get(3).copied().unwrap_or("").into(),
            });
        } else {
            meta.insert(k, v.to_string());
        }
    }
    Some(Recipe {
        id: id.to_string(),
        name: meta.get("namn").cloned().unwrap_or_else(|| id.to_string()),
        category: meta.get("kategori").cloned().unwrap_or_else(|| "Övrigt".into()),
        kind: meta.get("typ").cloned().unwrap_or_else(|| "läs".into()),
        needs: meta.get("kraver").or(meta.get("krav")).cloned().unwrap_or_default(),
        desc: meta.get("beskrivning").cloned().unwrap_or_default(),
        params,
    })
}

/// Enkel CSV-läsare (citattecken, "" i citat, , eller ; som avgränsare).
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let text = text.trim_start_matches('\u{feff}');
    let first = text.lines().next().unwrap_or("");
    let sep = if first.matches(';').count() > first.matches(',').count() { ';' } else { ',' };
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut q = false;
    let mut it = text.chars().peekable();
    while let Some(ch) = it.next() {
        match ch {
            '"' if q && it.peek() == Some(&'"') => {
                cell.push('"');
                it.next();
            }
            '"' => q = !q,
            c if c == sep && !q => row.push(std::mem::take(&mut cell)),
            '\r' if !q => {}
            '\n' if !q => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            c => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

pub fn find_csv_path(lines: &[String]) -> Option<PathBuf> {
    let re = regex::Regex::new(r#"((?:[A-Za-z]:\\|/|~/)[^"']*?\.csv)"#).ok()?;
    lines.iter().rev().find_map(|l| {
        re.captures(l).map(|c| {
            let p = &c[1];
            match p.strip_prefix("~/") {
                Some(rest) => core::home().join(rest),
                None => PathBuf::from(p),
            }
        })
    })
}

#[derive(Default)]
pub struct PuppaPage {
    recipes: Vec<Recipe>,
    tenants: Vec<String>,
    active: String,
    loaded: bool,
    search: String,
    sel: Option<String>,
    values: HashMap<String, String>,
    confirm: bool,
    job: Option<Job>,
    job_open: bool,
    result: Option<(PathBuf, Vec<Vec<String>>)>,
    check_result: bool,
}

impl PuppaPage {
    fn load(&mut self) {
        let mut rs: BTreeMap<String, Recipe> = BTreeMap::new();
        for dir in [core::cousin_dir("puppa").join("recipes"), core::home().join(".puppa").join("recipes")] {
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.extension().is_some_and(|x| x == "ps1") {
                        let id = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        if let Some(r) = std::fs::read_to_string(&p).ok().and_then(|t| parse_recipe(&id, &t)) {
                            rs.insert(id, r);
                        }
                    }
                }
            }
        }
        let order =
            ["SharePoint", "OneDrive", "Exchange", "Grupper", "Användare", "Teams", "Entra ID", "Excel", "Lokalt"];
        let mut v: Vec<Recipe> = rs.into_values().collect();
        v.sort_by_key(|r| {
            (order.iter().position(|c| *c == r.category).unwrap_or(50), r.category.clone(), r.writes(), r.name.clone())
        });
        self.recipes = v;
        let dir = core::home().join(".puppa");
        let kund: serde_json::Value = std::fs::read_to_string(dir.join("kunder.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        self.tenants = kund["tenants"]
            .as_array()
            .map(|a| a.iter().filter_map(|t| t["name"].as_str().map(String::from)).collect())
            .unwrap_or_default();
        let cfg: serde_json::Value = std::fs::read_to_string(dir.join("config.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        self.active = cfg["active"].as_str().unwrap_or("").to_string();
        self.loaded = true;
    }

    fn start(&mut self, ctx: &egui::Context, args: Vec<String>, title: String) -> Option<Action> {
        match Job::start("puppa", &args, title, ctx) {
            Ok(j) => {
                self.job = Some(j);
                self.job_open = false;
                self.check_result = true;
                None
            }
            Err(e) => Some(Action::Toast(e)),
        }
    }

    fn run_args(&self, r: &Recipe) -> Vec<String> {
        let mut a = vec!["kör".to_string(), r.id.clone()];
        for p in &r.params {
            let v = self.values.get(&p.name).cloned().unwrap_or_default();
            if !v.trim().is_empty() {
                a.push(format!("{}={}", p.name, v.trim()));
            }
        }
        a.push("--csv".into());
        if r.writes() {
            a.push("--ja".into());
        }
        a
    }

    pub fn ui(&mut self, ui: &mut Ui, ctx: &egui::Context) -> Option<Action> {
        if !self.loaded {
            self.load();
        }
        let mut act = None;
        let busy = self.job.as_ref().is_some_and(|j| j.finished().is_none());
        if self.check_result
            && let Some(ok) = self.job.as_ref().and_then(|j| j.finished())
        {
            self.check_result = false;
            self.load(); // en inloggning kan ha lagt till en tenant
            if ok && let Some(p) = find_csv_path(&self.job.as_ref().map(|j| j.text()).unwrap_or_default()) {
                let rows = std::fs::read_to_string(&p).map(|t| parse_csv(&t)).unwrap_or_default();
                self.result = Some((p, rows));
            }
        }
        let accent = xterm(75);

        // tenant-rad
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("TENANT").color(DIM).size(11.0).extra_letter_spacing(0.8));
                    let shown = if self.active.is_empty() { "Ingen vald".to_string() } else { self.active.clone() };
                    let mut pick = None;
                    egui::ComboBox::from_id_salt("tenant")
                        .selected_text(RichText::new(shown).size(15.0))
                        .width(280.0)
                        .show_ui(ui, |ui| {
                            for t in &self.tenants {
                                if ui.selectable_label(*t == self.active, t).clicked() {
                                    pick = Some(t.clone());
                                }
                            }
                            if self.tenants.is_empty() {
                                ui.label(RichText::new("Listan fylls på när du loggar in i ett recept.").color(DIM));
                            }
                        });
                    if let Some(t) = pick {
                        act = self.start(ctx, vec!["tenant".into(), t.clone()], format!("Växlar till {t}"));
                        self.active = t;
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::secondary(ui, true, "Öppna i terminal").clicked() {
                        act = Some(Action::Terminal(vec![]));
                    }
                    if theme::secondary(ui, !busy, "Logga ut").on_hover_text("PnP, Exchange, Teams och Graph").clicked()
                    {
                        act = self.start(ctx, vec!["logga-ut".into()], "Loggar ut".into());
                    }
                });
            });
        });
        ui.add_space(12.0);

        let avail = ui.available_width();
        let list_w = (avail * 0.34).clamp(240.0, 360.0);
        ui.horizontal_top(|ui| {
            // receptlistan
            ui.vertical(|ui| {
                ui.set_width(list_w);
                field(ui, &mut self.search, "Sök bland recepten…", list_w);
                ui.add_space(4.0);
                egui::ScrollArea::vertical().id_salt("recept").auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let q = self.search.to_lowercase();
                    let shown: Vec<Recipe> = self
                        .recipes
                        .iter()
                        .filter(|r| {
                            q.is_empty() || format!("{} {} {}", r.name, r.category, r.desc).to_lowercase().contains(&q)
                        })
                        .cloned()
                        .collect();
                    if shown.is_empty() {
                        ui.label(RichText::new("Inga recept matchar.").color(DIM));
                    }
                    let mut cat = String::new();
                    for r in &shown {
                        if r.category != cat {
                            cat = r.category.clone();
                            let n = shown.iter().filter(|x| x.category == cat).count();
                            theme::overline(ui, &format!("{cat} · {n}"));
                        }
                        let on = self.sel.as_deref() == Some(&r.id);
                        let dot = if r.writes() { theme::WRITE } else { GREEN.gamma_multiply(0.8) };
                        if theme::list_row(ui, on, &r.name, Some(dot), accent) && !on {
                            self.sel = Some(r.id.clone());
                            self.values = r.params.iter().map(|p| (p.name.clone(), p.default.clone())).collect();
                            self.confirm = false;
                        }
                    }
                });
            });
            ui.add_space(14.0);
            // detalj
            ui.vertical(|ui| {
                let Some(r) = self.sel.as_ref().and_then(|id| self.recipes.iter().find(|r| &r.id == id)).cloned()
                else {
                    theme::card(ui, |ui| {
                        ui.label(RichText::new(format!("{} recept", self.recipes.len())).size(22.0));
                        ui.label(
                            RichText::new("Välj ett recept i listan för att se vad det gör och köra det.").color(DIM),
                        );
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            theme::badge(ui, "● läser", GREEN);
                            theme::badge(ui, "● ändrar i tenanten", theme::WRITE);
                        });
                    });
                    ui.add_space(8.0);
                    job_panel(ui, &self.job, &mut self.job_open);
                    return;
                };
                theme::card(ui, |ui| {
                    ui.label(RichText::new(&r.name).size(21.0));
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        let (t, c) = if r.writes() { ("ändrar", theme::WRITE) } else { ("läser", GREEN) };
                        theme::badge(ui, t, c);
                        theme::badge(ui, &r.category, DIM);
                        if !r.needs.is_empty() {
                            ui.label(RichText::new(format!("kräver {}", r.needs)).color(DIM).size(12.0));
                        }
                    });
                    if !r.desc.is_empty() {
                        ui.add_space(4.0);
                        ui.label(RichText::new(&r.desc).color(TEXT.gamma_multiply(0.9)));
                    }
                    if !r.params.is_empty() {
                        ui.add_space(10.0);
                        egui::Grid::new("params").num_columns(2).spacing(vec2(14.0, 10.0)).show(ui, |ui| {
                            for p in &r.params {
                                ui.label(RichText::new(&p.label).color(DIM));
                                let v = self.values.entry(p.name.clone()).or_default();
                                let hint = if p.lookup.starts_with('@') { "skriv namn eller e-post…" } else { "" };
                                field(ui, v, hint, 320.0);
                                ui.end_row();
                            }
                        });
                    }
                    if r.writes() {
                        ui.add_space(6.0);
                        theme::toggle(ui, &mut self.confirm, "Jag vet att det här ändrar i tenanten");
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let can = !busy && (!r.writes() || self.confirm);
                        if theme::primary(ui, can, "  Kör recept  ", accent.gamma_multiply(0.85)) {
                            let a = self.run_args(&r);
                            act = self.start(ctx, a, format!("Kör {}", r.name));
                            self.result = None;
                        }
                        if theme::secondary(ui, true, "Kör i terminal")
                            .on_hover_text("för recept som frågar något eller behöver en inloggning")
                            .clicked()
                        {
                            let mut a = self.run_args(&r);
                            a.retain(|x| x != "--ja" || self.confirm);
                            act = Some(Action::Terminal(a));
                        }
                    });
                });
                ui.add_space(8.0);
                job_panel(ui, &self.job, &mut self.job_open);
                if let Some((p, rows)) = &self.result {
                    theme::card(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("Resultat · {} rader", rows.len().saturating_sub(1))).size(16.0),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if theme::secondary(ui, true, "Öppna filen").clicked() {
                                    open_path(p);
                                }
                            });
                        });
                        egui::ScrollArea::both().id_salt("resultat").max_height(ui.available_height().max(160.0)).show(
                            ui,
                            |ui| {
                                egui::Grid::new("csv").striped(true).spacing(vec2(18.0, 6.0)).show(ui, |ui| {
                                    for (i, row) in rows.iter().take(1000).enumerate() {
                                        for c in row {
                                            if i == 0 {
                                                ui.label(RichText::new(c).color(DIM).size(12.5));
                                            } else {
                                                ui.label(c);
                                            }
                                        }
                                        ui.end_row();
                                    }
                                });
                            },
                        );
                    });
                }
            });
        });
        act
    }
}

// ───────────────────────────── Muppa ─────────────────────────────

const FOLDERS: [(&str, &str, bool); 10] = [
    ("skrivbord", "Skrivbord", true),
    ("dokument", "Dokument", true),
    ("bilder", "Bilder", true),
    ("hamtade", "Hämtade filer", true),
    ("musik", "Musik", false),
    ("videor", "Videor", false),
    ("favoriter", "Favoriter", false),
    ("kontakter", "Kontakter", false),
    ("sticky", "Sticky Notes", false),
    ("signaturer", "Outlook-signaturer", false),
];
const BITLOCKER: [(&str, &str, &str); 4] = [
    ("pausa", "Spara nyckeln och pausa", "Rekommenderas. Slås på igen efter Entra-anslutningen."),
    ("stang-av", "Spara nyckeln och stäng av", "Dekrypterar hela disken."),
    ("spara", "Bara spara nyckeln", "Rör inte skyddet."),
    ("ingen", "Rör inte BitLocker", ""),
];
const DOMAIN: [(&str, &str, &str); 4] = [
    ("bada", "Lämna domänen och anslut till Entra ID", "Hela flytten"),
    ("doman", "Bara lämna domänen", ""),
    ("entra", "Bara anslut till Entra ID", "Datorn är redan utanför domänen"),
    ("inget", "Inget av dem", "Bara kopiering och förberedelser"),
];
const CLOUD: [(&str, &str, &str); 2] = [
    ("hoppa", "Hoppa över molnfiler", "Rekommenderas: de finns redan i OneDrive"),
    ("kopiera", "Försök kopiera dem också", "Laddar ner dem om OneDrive körs"),
];
const OPTIONAL: [(&str, &str); 6] = [
    ("kund", "Kund"),
    ("ansvarig", "Ansvarig"),
    ("dator", "Datorns namn"),
    ("upn", "E-post (UPN) i Entra ID"),
    ("tekniker", "Tekniker"),
    ("arende", "Ärendenummer"),
];

pub struct MuppaPage {
    user: String,
    target: String,
    folders: HashSet<&'static str>,
    custom: String,
    cloud: &'static str,
    edge: bool,
    chrome: bool,
    bitlocker: &'static str,
    domain: &'static str,
    admin: bool,
    admin_name: String,
    optional: HashMap<&'static str, String>,
    job: Option<Job>,
    job_open: bool,
    check_result: bool,
    result: Option<(PathBuf, PathBuf, String)>,
}

impl Default for MuppaPage {
    fn default() -> Self {
        MuppaPage {
            user: String::new(),
            target: r"C:\temp\{användare}".into(),
            folders: FOLDERS.iter().filter(|f| f.2).map(|f| f.0).collect(),
            custom: String::new(),
            cloud: "hoppa",
            edge: true,
            chrome: false,
            bitlocker: "pausa",
            domain: "bada",
            admin: true,
            admin_name: "lokaladmin".into(),
            optional: HashMap::new(),
            job: None,
            job_open: false,
            check_result: false,
            result: None,
        }
    }
}

pub fn muppa_out_dir() -> PathBuf {
    std::env::var_os("MUPPA_UT").map(PathBuf::from).unwrap_or_else(|| core::home().join("Documents").join("Muppa"))
}

impl MuppaPage {
    pub fn args(&self) -> Vec<String> {
        let mut a = vec!["skapa".to_string(), self.user.trim().to_string()];
        let mappar: Vec<&str> = FOLDERS.iter().map(|f| f.0).filter(|f| self.folders.contains(f)).collect();
        a.extend(["--mappar".into(), mappar.join(",")]);
        for p in self.custom.split([';', '\n']).map(str::trim).filter(|s| !s.is_empty()) {
            a.extend(["--egen".into(), p.to_string()]);
        }
        let mut wb = Vec::new();
        if self.edge {
            wb.push("edge");
        }
        if self.chrome {
            wb.push("chrome");
        }
        a.extend(["--webblasare".into(), wb.join(",")]);
        a.extend(["--bitlocker".into(), self.bitlocker.into()]);
        a.extend(["--doman".into(), self.domain.into()]);
        a.extend(["--molnfiler".into(), self.cloud.into()]);
        if self.admin && !self.admin_name.trim().is_empty() {
            a.extend(["--admin".into(), self.admin_name.trim().into()]);
        } else {
            a.push("--ingen-admin".into());
        }
        if !self.target.trim().is_empty() {
            a.extend(["--malmapp".into(), self.target.trim().into()]);
        }
        for (k, _) in OPTIONAL {
            if let Some(v) = self.optional.get(k).filter(|v| !v.trim().is_empty()) {
                a.extend([format!("--{k}"), v.trim().into()]);
            }
        }
        a
    }

    fn find_result(&self) -> Option<(PathBuf, PathBuf, String)> {
        let dir = muppa_out_dir();
        let user = self.user.trim().to_lowercase();
        let newest = |prefix: &str, ext: &str| -> Option<PathBuf> {
            std::fs::read_dir(&dir)
                .ok()?
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    let n = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    n.starts_with(prefix) && n.ends_with(ext) && n.to_lowercase().contains(&user)
                })
                .max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok())
        };
        let script = newest("Migrera-", ".ps1")?;
        let list = newest("Checklista-", ".md")?;
        let text = std::fs::read_to_string(&list).unwrap_or_default();
        Some((script, list, text))
    }

    pub fn ui(&mut self, ui: &mut Ui, ctx: &egui::Context) -> Option<Action> {
        let mut act = None;
        let busy = self.job.as_ref().is_some_and(|j| j.finished().is_none());
        if self.check_result
            && let Some(ok) = self.job.as_ref().and_then(|j| j.finished())
        {
            self.check_result = false;
            self.result = if ok { self.find_result() } else { None };
            self.job_open = !ok;
        }
        let accent = xterm(170);
        egui::ScrollArea::vertical().id_salt("muppa").auto_shrink([false, false]).show(ui, |ui| {
            section_sub(ui, "Vem flyttas?", "Användaren vars profil flyttas från lokal AD till Entra ID", |ui| {
                egui::Grid::new("vem").num_columns(2).spacing(vec2(14.0, 10.0)).show(ui, |ui| {
                    ui.label(RichText::new("Användare").color(DIM));
                    field(ui, &mut self.user, "t.ex. anna (profilmappen i C:\\Users)", 340.0);
                    ui.end_row();
                    ui.label(RichText::new("Målmapp").color(DIM));
                    field(ui, &mut self.target, "", 340.0);
                    ui.end_row();
                });
            });
            section_sub(
                ui,
                "Mappar som kopieras",
                "Klicka för att välja. Omdirigerade OneDrive-mappar följs.",
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                        for (id, label, _) in FOLDERS {
                            let mut on = self.folders.contains(id);
                            if theme::chip(ui, &mut on, label, accent) {
                                if on {
                                    self.folders.insert(id);
                                } else {
                                    self.folders.remove(id);
                                }
                            }
                        }
                    });
                    ui.add_space(6.0);
                    field(ui, &mut self.custom, "Egna sökvägar, separera med ;", f32::INFINITY);
                    ui.add_space(10.0);
                    ui.label(RichText::new("Filer som bara finns i OneDrive-molnet").color(DIM).size(12.5));
                    ui.columns(2, |c| {
                        for (i, (id, label, desc)) in CLOUD.iter().enumerate() {
                            if theme::option_card(&mut c[i], self.cloud == *id, label, desc, accent) {
                                self.cloud = id;
                            }
                        }
                    });
                },
            );
            ui.columns(2, |cols| {
                section(&mut cols[0], "BitLocker", |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    for (id, label, desc) in BITLOCKER {
                        if theme::option_card(ui, self.bitlocker == id, label, desc, accent) {
                            self.bitlocker = id;
                        }
                    }
                });
                section(&mut cols[1], "Domän och Entra ID", |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    for (id, label, desc) in DOMAIN {
                        if theme::option_card(ui, self.domain == id, label, desc, accent) {
                            self.domain = id;
                        }
                    }
                });
            });
            ui.columns(2, |cols| {
                section_sub(&mut cols[0], "Webbläsare", "Bokmärken exporteras, lösenord sparar du själv", |ui| {
                    theme::toggle(ui, &mut self.edge, "Microsoft Edge");
                    theme::toggle(ui, &mut self.chrome, "Google Chrome");
                });
                section_sub(&mut cols[1], "Lokalt admin-konto", "Lösenordet frågas efter när skriptet körs", |ui| {
                    theme::toggle(ui, &mut self.admin, "Skapa ett lokalt admin-konto");
                    ui.add_enabled_ui(self.admin, |ui| field(ui, &mut self.admin_name, "kontonamn", 220.0));
                });
            });
            section_sub(ui, "Valfria uppgifter", "Hamnar i checklistan", |ui| {
                egui::Grid::new("valfritt").num_columns(4).spacing(vec2(14.0, 10.0)).show(ui, |ui| {
                    for (i, (k, label)) in OPTIONAL.iter().enumerate() {
                        ui.label(RichText::new(*label).color(DIM));
                        field(ui, self.optional.entry(k).or_default(), "", 220.0);
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            });
            theme::card(ui, |ui| {
                ui.horizontal(|ui| {
                    let ok = !self.user.trim().is_empty() && !busy;
                    if theme::primary(ui, ok, "  Skapa skript och checklista  ", accent.gamma_multiply(0.85)) {
                        match Job::start("muppa", &self.args(), format!("Skapar skript för {}", self.user.trim()), ctx)
                        {
                            Ok(j) => {
                                self.job = Some(j);
                                self.check_result = true;
                                self.job_open = false;
                                self.result = None;
                            }
                            Err(e) => act = Some(Action::Toast(e)),
                        }
                    }
                    if self.user.trim().is_empty() {
                        ui.label(RichText::new("Fyll i användaren först.").color(DIM));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::secondary(ui, true, "Guiden i terminal").clicked() {
                            act = Some(Action::Terminal(vec!["ny".into()]));
                        }
                        if theme::secondary(ui, true, "Öppna mappen").clicked() {
                            let d = muppa_out_dir();
                            let _ = std::fs::create_dir_all(&d);
                            open_path(&d);
                        }
                    });
                });
            });
            ui.add_space(8.0);
            job_panel(ui, &self.job, &mut self.job_open);
            if let Some((script, list, text)) = &self.result {
                section_sub(ui, "Klart!", &format!("{}  ·  {}", script.display(), list.display()), |ui| {
                    ui.horizontal(|ui| {
                        if theme::primary(ui, true, "Visa skriptet", accent.gamma_multiply(0.85)) {
                            open_path(script);
                        }
                        if theme::secondary(ui, true, "Öppna checklistan").clicked() {
                            open_path(list);
                        }
                        if theme::secondary(ui, true, "Öppna mappen").clicked() {
                            open_path(script.parent().unwrap_or(Path::new(".")));
                        }
                    });
                    ui.add_space(8.0);
                    egui::Frame::new()
                        .fill(Color32::from_rgb(0x0e, 0x11, 0x17))
                        .corner_radius(10)
                        .inner_margin(14)
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical().id_salt("check").max_height(340.0).show(ui, |ui| {
                                for l in text.lines() {
                                    let t = l.trim_start();
                                    if let Some(h) = t.strip_prefix("## ").or(t.strip_prefix("# ")) {
                                        ui.add_space(6.0);
                                        ui.label(RichText::new(h).size(16.0));
                                    } else if let Some(item) = t.strip_prefix("- [ ] ") {
                                        ui.label(format!("☐   {item}"));
                                    } else if !t.is_empty() {
                                        ui.label(RichText::new(l).color(TEXT.gamma_multiply(0.85)));
                                    }
                                }
                            });
                        });
                });
            }
        });
        act
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_header() {
        let t = "<#PUPPA\nnamn:        Information om en användare\nkategori:    Användare\ntyp:         läs\nkräver:      graph:User.Read.All\nbeskrivning: Konto och licenser.\nparam:       Anvandare | Användare (UPN) | | @user\n#>\n$u = 1";
        let r = parse_recipe("anvandare-info", t).unwrap();
        assert_eq!(r.name, "Information om en användare");
        assert_eq!(r.category, "Användare");
        assert_eq!(r.needs, "graph:User.Read.All");
        assert!(!r.writes());
        assert_eq!(
            r.params,
            vec![RParam {
                name: "Anvandare".into(),
                label: "Användare (UPN)".into(),
                default: "".into(),
                lookup: "@user".into()
            }]
        );
        assert!(parse_recipe("x", "inget huvud").is_none());
    }

    #[test]
    fn csv() {
        let rows = parse_csv("\u{feff}Namn,UPN\r\n\"Anna, A\",anna@x.se\n\"Säger \"\"hej\"\"\",b\n");
        assert_eq!(rows, vec![vec!["Namn", "UPN"], vec!["Anna, A", "anna@x.se"], vec!["Säger \"hej\"", "b"]]);
        assert_eq!(parse_csv("a;b\n1;2"), vec![vec!["a", "b"], vec!["1", "2"]]);
        let p = find_csv_path(&["✔ Sparade /Users/a/Documents/Puppa/rapport 1.csv".into()]);
        assert_eq!(p, Some(PathBuf::from("/Users/a/Documents/Puppa/rapport 1.csv")));
    }

    #[test]
    fn uppa_cache() {
        let t = r#"{"sources":{"brew":{"items":[{"source":"brew","name":"azure-cli","current":"2.1","latest":"2.2","label":"azure-cli","manual":false,"note":"","meta":{}}],"summary":"x","error":""}},"time":5}"#;
        let (items, src, time) = parse_uppa_cache(t);
        assert_eq!(items.len(), 1);
        assert!(items[0].outdated());
        assert_eq!(src["brew"].0, "x");
        assert_eq!(time, 5.0);
    }

    #[test]
    fn muppa_args() {
        let mut m = MuppaPage { user: "anna".into(), chrome: true, ..Default::default() };
        m.optional.insert("kund", "Acme".into());
        let a = m.args();
        assert_eq!(&a[..2], &["skapa", "anna"]);
        let s = a.join(" ");
        assert!(s.contains("--mappar skrivbord,dokument,bilder,hamtade"));
        assert!(s.contains("--webblasare edge,chrome"));
        assert!(s.contains("--admin lokaladmin"));
        assert!(s.contains("--kund Acme"));
        m.admin = false;
        assert!(m.args().contains(&"--ingen-admin".to_string()));
    }
}
