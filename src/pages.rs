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
        Self::start_many(name, vec![args.to_vec()], title, ctx)
    }

    /// Kör kusinen flera gånger efter varandra (t.ex. ett skript per vald användare) i ett och samma jobb.
    /// Jobbet lyckas bara om alla körningar lyckas; en misslyckad körning stoppar inte resten.
    pub fn start_many(
        name: &str,
        runs: Vec<Vec<String>>,
        title: impl Into<String>,
        ctx: &egui::Context,
    ) -> Result<Job, String> {
        let script = core::cousin_dir(name).join(format!("{name}.py"));
        if !script.exists() {
            return Err(tr!(format!("{name} is not installed"), format!("{name} är inte installerad")));
        }
        let py = core::find_python().ok_or(tr!("found no Python 3.8+", "hittar ingen Python 3.8+"))?;
        let lines = Arc::new(Mutex::new(Vec::new()));
        let done = Arc::new(Mutex::new(None));
        let (l2, d2, c2, name) = (lines.clone(), done.clone(), ctx.clone(), name.to_string());
        thread::spawn(move || {
            let mut all_ok = true;
            let n = runs.len();
            for (i, args) in runs.into_iter().enumerate() {
                if let Ok(mut l) = l2.lock() {
                    if n > 1 {
                        l.push(format!("── {}/{n} ──", i + 1));
                    }
                    l.push(format!("$ {name} {}", args.join(" ")));
                }
                all_ok &= run_one(&py, &script, &args, &l2, &c2);
            }
            if let Ok(mut d) = d2.lock() {
                *d = Some(all_ok);
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

/// En körning av kusinen; varje rad utdata (stdout + stderr, utan ANSI) läggs i `lines`. True = lyckades.
fn run_one(
    py: &[String],
    script: &Path,
    args: &[String],
    lines: &Arc<Mutex<Vec<String>>>,
    ctx: &egui::Context,
) -> bool {
    let mut cmd = Command::new(&py[0]);
    cmd.args(&py[1..])
        .arg(script)
        .args(args)
        .env("NO_COLOR", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .env("UPPA_NO_ANIM", "1")
        .env("TRUPPA_LANG", crate::core::lang())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            if let Ok(mut l) = lines.lock() {
                l.push(tr!(format!("✘ could not start: {e}"), format!("✘ kunde inte starta: {e}")));
            }
            return false;
        }
    };
    let outs: Vec<Box<dyn Read + Send>> =
        vec![Box::new(child.stdout.take().expect("stdout")), Box::new(child.stderr.take().expect("stderr"))];
    let mut readers = Vec::new();
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
    for r in readers {
        let _ = r.join();
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
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
                if theme::ghost(
                    ui,
                    if *open { tr!("Hide details", "Dölj detaljer") } else { tr!("Show details", "Visa detaljer") },
                ) {
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

/// (key, English, Swedish)
const SOURCE_TITLES: [(&str, &str, &str); 9] = [
    ("self", "Uppa", "Uppa"),
    ("brew", "Homebrew", "Homebrew"),
    ("winget", "winget", "winget"),
    ("pwsh", "PowerShell modules", "PowerShell-moduler"),
    ("az", "az extensions", "az-tillägg"),
    ("npm", "npm (Microsoft packages)", "npm (Microsoft-paket)"),
    ("dotnet", ".NET tools", ".NET-verktyg"),
    ("puppa", "Puppa", "Puppa"),
    ("muppa", "Muppa", "Muppa"),
];

fn source_title(k: &str) -> String {
    SOURCE_TITLES
        .iter()
        .find(|(a, _, _)| *a == k)
        .map(|(_, en, sv)| tr!(*en, *sv).to_string())
        .unwrap_or_else(|| k.to_string())
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
    /// grundpaketen ur Uppas BUILTIN_PROFILES: (namn, beskrivning, innehåll)
    profiles: Vec<(String, String, Vec<String>)>,
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
        if self.profiles.is_empty()
            && let Some(v) = core::describe("uppa", &["BUILTIN_PROFILES"])
        {
            self.profiles = parse_profiles(&v["BUILTIN_PROFILES"]);
        }
    }

    /// Läs om allt (efter en uppdatering av Uppa).
    pub fn reset(&mut self) {
        self.loaded = false;
        self.profiles.clear();
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
                act = self.run(
                    ctx,
                    &["status", "--refresh", "--quiet"],
                    tr!("Checking for updates", "Kollar efter uppdateringar"),
                );
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
                        (tr!("Not scanned yet", "Inte skannat än").to_string(), DIM)
                    } else if n_outdated == 0 {
                        (tr!("Everything is up to date", "Allt är uppdaterat").to_string(), GREEN)
                    } else {
                        (tr!(format!("{n_outdated} can be updated"), format!("{n_outdated} kan uppdateras")), YELLOW)
                    };
                    ui.label(RichText::new(big).size(22.0).color(col));
                    let sub = if self.time > 0.0 {
                        tr!(
                            format!(
                                "{} tools and modules · last checked {}",
                                self.items.len(),
                                ago(now_secs() - self.time)
                            ),
                            format!(
                                "{} verktyg och moduler · senast kollat {}",
                                self.items.len(),
                                ago(now_secs() - self.time)
                            )
                        )
                    } else {
                        tr!(
                            "Uppa checks Homebrew/winget, PowerShell modules, az, npm and .NET",
                            "Uppa kollar Homebrew/winget, PowerShell-moduler, az, npm och .NET"
                        )
                        .into()
                    };
                    ui.label(RichText::new(sub).color(DIM).size(12.5));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if chosen.is_empty() {
                        tr!("Update selected", "Uppdatera valda").to_string()
                    } else {
                        tr!(
                            format!("Update selected ({})", chosen.len()),
                            format!("Uppdatera valda ({})", chosen.len())
                        )
                    };
                    if theme::primary(ui, !busy && !chosen.is_empty(), &label, accent) {
                        let mut a: Vec<&str> = vec!["update", "-y"];
                        a.extend(chosen.iter().map(|s| s.as_str()));
                        act = self.run(ctx, &a, &format!("Uppdaterar {} st", chosen.len()));
                    }
                    if theme::secondary(ui, !busy, tr!("Check again", "Kolla igen")).clicked() {
                        act = self.run(
                            ctx,
                            &["status", "--refresh", "--quiet"],
                            tr!("Checking for updates", "Kollar efter uppdateringar"),
                        );
                    }
                });
            });
        });
        ui.add_space(8.0);
        job_panel(ui, &self.job, &mut self.job_open);

        egui::ScrollArea::vertical().id_salt("uppa-lista").auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                theme::overline(ui, tr!("What you have installed", "Det du har installerat"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    theme::toggle(
                        ui,
                        &mut self.show_all,
                        tr!("Also show what is up to date", "Visa även det som är aktuellt"),
                    );
                });
            });
            if self.items.is_empty() && busy {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label(
                        RichText::new(tr!(
                            "Uppa is scanning your tools. It takes half a minute the first time.",
                            "Uppa skannar dina verktyg. Det tar en halv minut första gången."
                        ))
                        .color(DIM),
                    );
                });
            }
            let mut by_src: BTreeMap<usize, (String, Vec<UItem>)> = BTreeMap::new();
            for it in &self.items {
                if !self.show_all && !it.outdated() {
                    continue;
                }
                let order = SOURCE_TITLES.iter().position(|(a, _, _)| *a == it.source).unwrap_or(99);
                by_src.entry(order).or_insert_with(|| (it.source.clone(), Vec::new())).1.push(it.clone());
            }
            if by_src.is_empty() && !self.items.is_empty() {
                theme::card(ui, |ui| {
                    ui.label(
                        RichText::new(tr!("✔  Nothing to update right now.", "✔  Inget att uppdatera just nu."))
                            .color(GREEN)
                            .size(15.0),
                    );
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
                                    tr!(
                                        format!("updated by hand. {}", it.note),
                                        format!("uppdateras för hand. {}", it.note)
                                    )
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
                section_sub(
                    &mut c[0],
                    tr!("Base packages", "Grundpaket"),
                    tr!("Install what a particular job needs", "Installera det som behövs för ett visst jobb"),
                    |ui| {
                        let profiles = if self.profiles.is_empty() {
                            ["bas", "m365", "azure", "alla"]
                                .iter()
                                .map(|k| (k.to_string(), String::new(), vec![]))
                                .collect()
                        } else {
                            self.profiles.clone()
                        };
                        ui.horizontal_wrapped(|ui| {
                            for (k, desc, items) in &profiles {
                                let label = profile_label(k);
                                let mut hover = desc.clone();
                                if !items.is_empty() {
                                    hover = format!("{hover}\n\n{}", items.join(", "));
                                }
                                let b = theme::secondary(ui, !busy, &label);
                                let b = if hover.trim().is_empty() { b } else { b.on_hover_text(hover.trim()) };
                                if b.clicked() {
                                    act = self.run(
                                        ctx,
                                        &["install", "-y", k],
                                        &tr!(
                                            format!("Installing the base package {label}"),
                                            format!("Installerar grundpaketet {label}")
                                        ),
                                    );
                                }
                            }
                        });
                    },
                );
                section_sub(
                    &mut c[1],
                    tr!("Tools", "Verktyg"),
                    tr!("Sign-out, health check and clean-up", "Utloggning, hälsokoll och städning"),
                    |ui| {
                        ui.horizontal_wrapped(|ui| {
                            if theme::secondary(ui, !busy, tr!("Sign out of everything", "Logga ut från allt"))
                                .on_hover_text("Graph, Exchange, PnP, Az, az, m365 …")
                                .clicked()
                            {
                                act = self.run(
                                    ctx,
                                    &["logout", "-y"],
                                    tr!("Signing out of everything", "Loggar ut från allt"),
                                );
                            }
                            if theme::secondary(ui, !busy, tr!("Health check", "Hälsokoll")).clicked() {
                                act = self.run(ctx, &["doctor"], tr!("Health check", "Hälsokoll"));
                            }
                            if theme::secondary(ui, !busy, tr!("Clean up old versions", "Städa gamla versioner"))
                                .clicked()
                            {
                                act = self.run(
                                    ctx,
                                    &["clean", "-y"],
                                    tr!("Cleaning up old versions", "Städar gamla versioner"),
                                );
                            }
                            if theme::secondary(ui, true, tr!("Open in terminal", "Öppna i terminal")).clicked() {
                                act = Some(Action::Terminal(vec![]));
                            }
                        });
                    },
                );
            });
        });
        act
    }
}

/// Uppas BUILTIN_PROFILES: {"bas": ["beskrivning", ["verktyg", …]], …}
pub fn parse_profiles(v: &serde_json::Value) -> Vec<(String, String, Vec<String>)> {
    let order = ["bas", "m365", "azure", "alla"];
    let mut out: Vec<(String, String, Vec<String>)> = v
        .as_object()
        .map(|o| {
            o.iter()
                .map(|(k, x)| {
                    let desc = x.get(0).and_then(|d| d.as_str()).unwrap_or("").to_string();
                    let items = x
                        .get(1)
                        .and_then(|i| i.as_array())
                        .map(|a| a.iter().filter_map(|y| y.as_str().map(String::from)).collect())
                        .unwrap_or_default();
                    (k.clone(), desc, items)
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by_key(|p| (order.iter().position(|o| *o == p.0).unwrap_or(99), p.0.clone()));
    out
}

fn profile_label(k: &str) -> String {
    match k {
        "m365" => "M365".into(),
        "bas" => tr!("Base", "Bas").into(),
        "alla" => tr!("All", "Alla").into(),
        _ => {
            let mut c = k.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        }
    }
}

fn now_secs() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn ago(s: f64) -> String {
    let s = s.max(0.0) as u64;
    match s {
        0..=59 => "nyss".into(),
        60..=3599 => tr!(format!("{} min ago", s / 60), format!("för {} min sedan", s / 60)),
        3600..=86_399 => tr!(format!("{} h ago", s / 3600), format!("för {} h sedan", s / 3600)),
        _ => tr!(format!("{} days ago", s / 86_400), format!("för {} dagar sedan", s / 86_400)),
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
        name: (!core::is_sv())
            .then(|| meta.get("name_en").cloned())
            .flatten()
            .or(meta.get("namn").cloned())
            .unwrap_or_else(|| id.to_string()),
        category: meta.get("kategori").cloned().unwrap_or_else(|| "Övrigt".into()),
        kind: meta.get("typ").cloned().unwrap_or_else(|| "läs".into()),
        needs: meta.get("kraver").or(meta.get("krav")).cloned().unwrap_or_default(),
        desc: (!core::is_sv())
            .then(|| meta.get("description_en").cloned())
            .flatten()
            .or(meta.get("beskrivning").cloned())
            .unwrap_or_default(),
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

/// Områdena i Puppa: färg (släktens palett), ikonförkortning och vad de innehåller.
pub const AREAS: [(&str, u8, &str, &str, &str); 9] = [
    (
        "SharePoint",
        37,
        "SP",
        "Sajter, behörigheter, papperskorg och delning",
        "Sites, permissions, recycle bin and sharing",
    ),
    ("OneDrive", 75, "OD", "Hitta, ge åtkomst och se de största", "Find, give access and see the largest"),
    (
        "Exchange",
        33,
        "EX",
        "Delade postlådor, åtkomst, kalender och spårning",
        "Shared mailboxes, access, calendar and tracing",
    ),
    (
        "Grupper",
        177,
        "GR",
        "Medlemmar, lägg till och ta bort, grupper utan ägare",
        "Members, add and remove, groups without owners",
    ),
    (
        "Användare",
        49,
        "AN",
        "Info, licenser, gäster, MFA och spärra konton",
        "Info, licenses, guests, MFA and blocking accounts",
    ),
    ("Teams", 141, "TM", "Team, medlemmar och kanaler", "Teams, members and channels"),
    (
        "Entra ID",
        202,
        "ID",
        "Inloggningar, granskning, villkorlig åtkomst, enheter och roller",
        "Sign-ins, audit, Conditional Access, devices and roles",
    ),
    ("Excel", 71, "XL", "Exportera senaste resultatet", "Export the latest result"),
    ("Lokalt", 220, "LO", "Moduler och session", "Modules and session"),
];

/// English names for Puppa's areas and choices (Puppa's CAT_EN / CHOICE_EN), filled in when Puppa is loaded.
/// (CAT_EN, CHOICE_EN)
type PuppaEn = (HashMap<String, String>, HashMap<String, String>);
static PUPPA_EN: std::sync::RwLock<Option<PuppaEn>> = std::sync::RwLock::new(None);

/// An area's name in the chosen language (the Swedish name stays the key).
pub fn cat_label(cat: &str) -> String {
    if core::is_sv() {
        return cat.to_string();
    }
    PUPPA_EN
        .read()
        .ok()
        .and_then(|m| m.as_ref().and_then(|(c, _)| c.get(cat).cloned()))
        .unwrap_or_else(|| cat.to_string())
}

/// A choice's label in the chosen language (Puppa is still sent the Swedish value).
pub fn choice_label(v: &str) -> String {
    if core::is_sv() {
        return v.to_string();
    }
    PUPPA_EN.read().ok().and_then(|m| m.as_ref().and_then(|(_, c)| c.get(v).cloned())).unwrap_or_else(|| v.to_string())
}

/// Färg, ikon och beskrivning för ett område. Nya områden (som en uppdaterad Puppa har lagt till) får en färg ur
/// släktens palett och de två första bokstäverna som ikon. `hints` kommer från Puppas CAT_HINTS.
pub fn area_info(cat: &str, hints: &HashMap<String, String>) -> (Color32, String, String) {
    const PALETTE: [u8; 9] = [37, 75, 33, 177, 49, 141, 202, 71, 220];
    let known = AREAS.iter().find(|a| a.0 == cat);
    let color = known.map(|a| xterm(a.1)).unwrap_or_else(|| {
        let h = cat.bytes().fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize));
        xterm(PALETTE[h % PALETTE.len()])
    });
    let icon = known.map(|a| a.2.to_string()).unwrap_or_else(|| cat.chars().take(2).collect::<String>().to_uppercase());
    let hint = hints
        .get(cat)
        .map(|h| {
            let mut c = h.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .or_else(|| known.map(|a| tr!(a.4, a.3).to_string()))
        .unwrap_or_default();
    (color, icon, hint)
}

#[derive(Default)]
pub struct PuppaPage {
    recipes: Vec<Recipe>,
    tenants: Vec<String>,
    active: String,
    recent: Vec<String>,
    /// områdesbeskrivningar ur Puppas CAT_HINTS
    hints: HashMap<String, String>,
    loaded: bool,
    search: String,
    /// valt område (None = alla områden)
    area: Option<String>,
    sel: Option<String>,
    values: HashMap<String, String>,
    confirm: bool,
    job: Option<Job>,
    job_open: bool,
    result: Option<(PathBuf, Vec<Vec<String>>)>,
    check_result: bool,
    /// Användare-vyn (i stället för skriptvyn)
    users_mode: bool,
    users: crate::users::UsersView,
    /// valda användare som skriptet ska köras för (en körning per person om parametern bara tar en)
    run_for: Vec<String>,
    /// pågående uppslag för ett fält (parameter, jobb) och träffarna per fält
    lookup: Option<(String, Job)>,
    lookup_hits: HashMap<String, Vec<crate::users::Pick>>,
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
        let mut v: Vec<Recipe> = rs.into_values().collect();
        if !core::is_sv()
            && let Some(en) = core::describe("puppa", &["RECIPES_EN", "CAT_EN", "CHOICE_EN"])
        {
            let map = |k: &str| -> HashMap<String, String> {
                en[k]
                    .as_object()
                    .map(|o| o.iter().filter_map(|(a, b)| b.as_str().map(|b| (a.clone(), b.to_string()))).collect())
                    .unwrap_or_default()
            };
            if let Ok(mut w) = PUPPA_EN.write() {
                *w = Some((map("CAT_EN"), map("CHOICE_EN")));
            }
            for r in v.iter_mut() {
                if let Some(x) = en["RECIPES_EN"][&r.id].as_array() {
                    if let Some(n) = x.first().and_then(|n| n.as_str()) {
                        r.name = n.to_string();
                    }
                    if let Some(d) = x.get(1).and_then(|d| d.as_str()) {
                        r.desc = d.to_string();
                    }
                    if let Some(q) = x.get(2).and_then(|q| q.as_object()) {
                        for p in r.params.iter_mut() {
                            if let Some(l) =
                                q.get(&p.name).or_else(|| q.get(p.name.trim_end_matches('?'))).and_then(|l| l.as_str())
                            {
                                p.label = l.to_string();
                            }
                        }
                    }
                }
            }
        }
        v.sort_by_key(|r| {
            (AREAS.iter().position(|a| a.0 == r.category).unwrap_or(50), r.category.clone(), r.writes(), r.name.clone())
        });
        self.recipes = v;
        let dir = core::home().join(".puppa");
        let read = |f: &str| -> serde_json::Value {
            std::fs::read_to_string(dir.join(f)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
        };
        let kund = read("kunder.json");
        self.tenants = kund["tenants"]
            .as_array()
            .map(|a| a.iter().filter_map(|t| t["name"].as_str().map(String::from)).collect())
            .unwrap_or_default();
        let cfg = read("config.json");
        self.active = cfg["active"].as_str().unwrap_or("").to_string();
        self.recent = cfg["recent"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default();
        if let Some(v) = core::describe("puppa", &["CAT_HINTS"])
            && let Some(o) = v["CAT_HINTS"].as_object()
        {
            self.hints = o.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect();
        }
        // för skärmdumpar och tester: RUPPA_PUPPA_OPEN="Område/skript-id"
        if !self.loaded
            && let Ok(v) = std::env::var("RUPPA_PUPPA_OPEN")
        {
            if let Some(rest) = v.strip_prefix("users") {
                // users:upn,upn#skript-id  (för skärmdumpar och tester)
                self.users_mode = true;
                let (who, script) =
                    rest.trim_start_matches(':').split_once('#').unwrap_or((rest.trim_start_matches(':'), ""));
                self.users.chosen = who.split(',').filter(|x| !x.is_empty()).map(String::from).collect();
                if let Some(r) = self.recipes.iter().find(|r| r.id == script).cloned() {
                    let chosen = self.users.chosen.clone();
                    self.pick(&r);
                    if let Some((pname, _)) = crate::users::user_param(&r) {
                        self.values.insert(pname, chosen.join(","));
                    }
                    self.run_for = chosen;
                }
            }
            let (a, id) = if v.starts_with("users") { ("", "") } else { v.split_once('/').unwrap_or((&v, "")) };
            self.area = (!a.is_empty()).then(|| a.to_string());
            if let Some(r) = self.recipes.iter().find(|r| r.id == id).cloned() {
                self.pick(&r);
            }
        }
        self.loaded = true;
    }

    /// Läs om skript, områden och tenants (efter en uppdatering av Puppa).
    pub fn reset(&mut self) {
        self.loaded = false;
        self.users.reset();
    }

    fn area(&self, cat: &str) -> (Color32, String, String) {
        area_info(cat, &self.hints)
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

    fn pick(&mut self, r: &Recipe) {
        self.run_for.clear();
        if self.sel.as_deref() != Some(&r.id) {
            self.sel = Some(r.id.clone());
            self.values = r.params.iter().map(|p| (p.name.clone(), p.default.clone())).collect();
            self.confirm = false;
        }
    }

    fn script_badges(r: &Recipe) -> Vec<(String, Color32)> {
        let mut b = vec![if r.writes() {
            (tr!("changes", "ändrar").to_string(), theme::WRITE)
        } else {
            (tr!("reads", "läser").to_string(), GREEN)
        }];
        if !r.params.is_empty() {
            let n = r.params.len();
            b.push((tr!(format!("{n} field{}", if n == 1 { "" } else { "s" }), format!("{n} fält")), DIM));
        }
        b
    }

    /// Rutnät med skriptkort.
    fn script_grid(&mut self, ui: &mut Ui, list: &[Recipe]) {
        let sel = self.sel.clone();
        let hints = self.hints.clone();
        let mut clicked: Option<Recipe> = None;
        theme::grid(ui, list, 250.0, 140.0, |ui, r, size| {
            let (col, icon, _) = area_info(&r.category, &hints);
            let on = sel.as_deref() == Some(&r.id);
            if theme::tile(ui, size, col, &icon, &r.name, &r.desc, &Self::script_badges(r), on).clicked() {
                clicked = Some(r.clone());
            }
        });
        if let Some(r) = clicked {
            self.pick(&r);
        }
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
        self.poll_lookup();

        let mut mode = usize::from(self.users_mode);
        if theme::segmented(ui, &[tr!("Scripts", "Skript"), tr!("Users", "Användare")], &mut mode, accent) {
            self.users_mode = mode == 1;
            self.sel = None;
            self.run_for.clear();
        }
        ui.add_space(10.0);

        if self.users_mode {
            return self.users_ui(ui, ctx, busy, accent).or(act);
        }

        // verktygsrad: tenant · sök · knappar
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
                    egui::ComboBox::from_id_salt("tenant")
                        .selected_text(RichText::new(shown).size(15.0))
                        .width(240.0)
                        .show_ui(ui, |ui| {
                            for t in &self.tenants {
                                if ui.selectable_label(*t == self.active, t).clicked() {
                                    pick = Some(t.clone());
                                }
                            }
                            if self.tenants.is_empty() {
                                ui.label(
                                    RichText::new(tr!(
                                        "The list fills up when you sign in from a script.",
                                        "Listan fylls på när du loggar in i ett skript."
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
                    }
                });
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(tr!("SEARCH", "SÖK")).color(DIM).size(11.0).extra_letter_spacing(0.8));
                    let w = (ui.available_width() - 250.0).max(180.0);
                    field(
                        ui,
                        &mut self.search,
                        &tr!(
                            format!("Search {} scripts…", self.recipes.len()),
                            format!("Sök bland {} skript…", self.recipes.len())
                        ),
                        w,
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                    if theme::secondary(ui, true, "Terminal")
                        .on_hover_text(tr!("Puppa in the built-in terminal", "Puppa i den inbyggda terminalen"))
                        .clicked()
                    {
                        act = Some(Action::Terminal(vec![]));
                    }
                    if theme::secondary(ui, !busy, tr!("Sign out", "Logga ut"))
                        .on_hover_text(tr!("PnP, Exchange, Teams and Graph", "PnP, Exchange, Teams och Graph"))
                        .clicked()
                    {
                        act = self.start(ctx, vec!["logga-ut".into()], tr!("Signing out", "Loggar ut").into());
                    }
                });
            });
        });
        ui.add_space(14.0);

        let selected = self.sel.as_ref().and_then(|id| self.recipes.iter().find(|r| &r.id == id)).cloned();
        let avail = ui.available_width();
        let side = selected.is_some() && avail > 820.0;
        let detail_w = if side { (avail * 0.38).clamp(340.0, 440.0) } else { avail };
        let main_w = if side { avail - detail_w - 16.0 } else { avail };

        ui.horizontal_top(|ui| {
            // vänster: områden / skript
            ui.vertical(|ui| {
                ui.set_width(main_w);
                egui::ScrollArea::vertical().id_salt("puppa-main").auto_shrink([false, false]).show(ui, |ui| {
                    let q = self.search.trim().to_lowercase();
                    if !q.is_empty() {
                        // fuzzy-sök: namnet väger tyngst, sedan område och beskrivning
                        let hits: Vec<Recipe> = crate::fuzzy::rank(&q, &self.recipes, |r| {
                            format!("{} {} {} {}", r.name, r.category, cat_label(&r.category), r.desc)
                        })
                        .into_iter()
                        .cloned()
                        .collect();
                        theme::overline(ui, &tr!(format!("{} matches", hits.len()), format!("{} träffar", hits.len())));
                        if hits.is_empty() {
                            ui.label(
                                RichText::new(tr!(
                                    "No scripts match. Try another word.",
                                    "Inga skript matchar. Prova ett annat ord."
                                ))
                                .color(DIM),
                            );
                        }
                        self.script_grid(ui, &hits);
                    } else if let Some(cat) = self.area.clone() {
                        let (col, icon, hint) = self.area(&cat);
                        ui.horizontal(|ui| {
                            if theme::ghost(ui, tr!("← All areas", "← Alla områden")) {
                                self.area = None;
                            }
                        });
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
                                ui.label(RichText::new(cat_label(&cat)).size(22.0).color(TEXT));
                                ui.label(RichText::new(hint).color(DIM));
                            });
                        });
                        ui.add_space(10.0);
                        let list: Vec<Recipe> = self.recipes.iter().filter(|r| r.category == cat).cloned().collect();
                        let (reads, writes): (Vec<Recipe>, Vec<Recipe>) = list.into_iter().partition(|r| !r.writes());
                        if !reads.is_empty() {
                            theme::overline(
                                ui,
                                &tr!(
                                    format!("View and export · {}", reads.len()),
                                    format!("Visa och exportera · {}", reads.len())
                                ),
                            );
                            self.script_grid(ui, &reads);
                        }
                        if !writes.is_empty() {
                            theme::overline(
                                ui,
                                &tr!(format!("Change · {}", writes.len()), format!("Ändra · {}", writes.len())),
                            );
                            self.script_grid(ui, &writes);
                        }
                    } else {
                        let recent: Vec<Recipe> = self
                            .recent
                            .iter()
                            .filter_map(|id| self.recipes.iter().find(|r| &r.id == id))
                            .cloned()
                            .collect();
                        if !recent.is_empty() {
                            theme::overline(ui, tr!("Recently used", "Senast använda"));
                            self.script_grid(ui, &recent[..recent.len().min(3)]);
                        }
                        theme::overline(ui, tr!("Areas", "Områden"));
                        // alla områden som skripten själva anger, i Puppas ordning; nya hamnar sist
                        let mut names: Vec<String> = Vec::new();
                        for r in &self.recipes {
                            if !names.contains(&r.category) {
                                names.push(r.category.clone());
                            }
                        }
                        let cats: Vec<(String, usize, usize)> = names
                            .into_iter()
                            .map(|c| {
                                let n = self.recipes.iter().filter(|r| r.category == c).count();
                                let w = self.recipes.iter().filter(|r| r.category == c && r.writes()).count();
                                (c, n, w)
                            })
                            .collect();
                        let hints = self.hints.clone();
                        let mut open = None;
                        theme::grid(ui, &cats, 230.0, 146.0, |ui, (cat, n, w), size| {
                            let (col, icon, hint) = area_info(cat, &hints);
                            let mut badges = vec![(tr!(format!("{n} scripts"), format!("{n} skript")), col)];
                            if *w > 0 {
                                badges.push((tr!(format!("{w} change"), format!("{w} ändrar")), theme::WRITE));
                            }
                            if theme::tile(ui, size, col, &icon, &cat_label(cat), &hint, &badges, false).clicked() {
                                open = Some(cat.clone());
                            }
                        });
                        if let Some(c) = open {
                            self.area = Some(c);
                        }
                    }
                });
            });

            // höger: valt skript
            if let Some(r) = selected.clone().filter(|_| side) {
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    ui.set_width(detail_w);
                    egui::ScrollArea::vertical().id_salt("puppa-detail").auto_shrink([false, false]).show(ui, |ui| {
                        act = self.detail(ui, ctx, &r, busy, accent).or(act.take());
                    });
                });
            }
        });
        // smalt fönster: detaljen under listan
        if let Some(r) = selected.filter(|_| !side) {
            ui.add_space(10.0);
            act = self.detail(ui, ctx, &r, busy, accent).or(act);
        }
        act
    }

    /// Användare-vyn: listan och åtgärderna, eller det valda skriptet för de valda användarna.
    fn users_ui(&mut self, ui: &mut Ui, ctx: &egui::Context, busy: bool, accent: Color32) -> Option<Action> {
        let selected = self.sel.as_ref().and_then(|id| self.recipes.iter().find(|r| &r.id == id)).cloned();
        if let Some(r) = selected.filter(|_| !self.run_for.is_empty()) {
            if theme::ghost(ui, tr!("← Back to the users", "← Tillbaka till användarna")) {
                self.sel = None;
                self.run_for.clear();
                return None;
            }
            ui.add_space(6.0);
            let mut act = None;
            egui::ScrollArea::vertical().id_salt("anv-skript").auto_shrink([false, false]).show(ui, |ui| {
                act = self.detail(ui, ctx, &r, busy, accent);
            });
            return act;
        }
        let recipes = self.recipes.clone();
        match self.users.ui(ui, ctx, &recipes, accent) {
            Some(crate::users::UsersAction::Run(id)) => {
                if let Some(r) = recipes.iter().find(|r| r.id == id) {
                    let chosen = self.users.chosen.clone();
                    self.pick(r);
                    if let Some((pname, _)) = crate::users::user_param(r) {
                        self.values.insert(pname, chosen.join(","));
                    }
                    self.run_for = chosen;
                    self.result = None;
                }
                None
            }
            Some(crate::users::UsersAction::Page(a)) => Some(a),
            None => None,
        }
    }

    /// Ta hand om ett färdigt uppslag (grupp, team, sajt …) för ett fält.
    fn poll_lookup(&mut self) {
        let done = self.lookup.as_ref().is_some_and(|(_, j)| j.finished().is_some());
        if done && let Some((name, j)) = self.lookup.take() {
            let hits = crate::users::parse_lookup(&j.text()).map(|x| x.1).unwrap_or_default();
            self.lookup_hits.insert(name, hits);
        }
    }

    /// Körningarna för det valda skriptet: en per användare, eller en enda.
    fn runs_for(&self, r: &Recipe) -> Vec<Vec<String>> {
        match crate::users::user_param(r) {
            Some((pname, false)) if self.run_for.len() > 1 => self
                .run_for
                .iter()
                .map(|u| {
                    let mut me = PuppaPage { values: self.values.clone(), ..Default::default() };
                    me.values.insert(pname.clone(), u.clone());
                    me.run_args(r)
                })
                .collect(),
            _ => vec![self.run_args(r)],
        }
    }

    fn detail(&mut self, ui: &mut Ui, ctx: &egui::Context, r: &Recipe, busy: bool, accent: Color32) -> Option<Action> {
        let mut act = None;
        let (col, icon, _) = self.area(&r.category);
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
                ui.label(RichText::new(cat_label(&r.category)).color(DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::ghost(ui, tr!("Close", "Stäng")) {
                        self.sel = None;
                    }
                });
            });
            ui.add_space(6.0);
            ui.label(RichText::new(&r.name).size(21.0).color(TEXT));
            ui.horizontal_wrapped(|ui| {
                let (t, c) = if r.writes() {
                    (tr!("changes the tenant", "ändrar i tenanten"), theme::WRITE)
                } else {
                    (tr!("only reads", "läser bara"), GREEN)
                };
                theme::badge(ui, t, c);
                if !r.needs.is_empty() {
                    theme::badge(ui, &tr!(format!("needs {}", r.needs), format!("kräver {}", r.needs)), DIM);
                }
            });
            if !r.desc.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(&r.desc).color(TEXT.gamma_multiply(0.9)));
            }
            if !r.params.is_empty() {
                ui.add_space(12.0);
                let user_p = crate::users::user_param(r).map(|x| x.0);
                for p in &r.params {
                    ui.label(RichText::new(&p.label).color(DIM).size(12.5));
                    // valda användare: visas som namn, inte som ett fält
                    if !self.run_for.is_empty() && user_p.as_deref() == Some(&p.name) {
                        ui.horizontal_wrapped(|ui| {
                            for u in &self.run_for {
                                theme::badge(ui, &self.users.name_of(u), accent);
                            }
                        });
                        ui.add_space(4.0);
                        continue;
                    }
                    // fjärde fältet i Puppas param-rad: @uppslag (sök i tenanten) eller fasta val ("Medlem,Ägare")
                    let is_lookup = p.lookup.starts_with('@');
                    let kind = if is_lookup {
                        p.lookup.trim_start_matches('@').trim_end_matches('+').to_string()
                    } else {
                        String::new()
                    };
                    let many = p.lookup.ends_with('+');
                    let choices: Vec<String> = if is_lookup {
                        vec![]
                    } else {
                        p.lookup.split(',').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect()
                    };
                    if !choices.is_empty() {
                        let v = self.values.entry(p.name.clone()).or_default();
                        if v.is_empty() {
                            *v = p.default.clone();
                        }
                        let mut cur = choices.iter().position(|c| c.eq_ignore_ascii_case(v)).unwrap_or(0);
                        let labels: Vec<String> = choices.iter().map(|c| choice_label(c)).collect();
                        let opts: Vec<&str> = labels.iter().map(|c| c.as_str()).collect();
                        theme::segmented(ui, &opts, &mut cur, accent);
                        *v = choices[cur].clone();
                    } else if kind.is_empty() {
                        field(ui, self.values.entry(p.name.clone()).or_default(), "", f32::INFINITY);
                    } else {
                        let searching = self.lookup.as_ref().is_some_and(|(n, _)| n == &p.name);
                        ui.horizontal(|ui| {
                            let v = self.values.entry(p.name.clone()).or_default();
                            field(
                                ui,
                                v,
                                tr!("type part of the name and search …", "skriv en del av namnet och sök …"),
                                (ui.available_width() - 80.0).max(120.0),
                            );
                            if searching {
                                ui.add(egui::Spinner::new().color(accent));
                            } else if theme::secondary(ui, !busy, tr!("Search", "Sök")).clicked() {
                                let q = v.rsplit(',').next().unwrap_or("").trim().to_string();
                                let args = vec!["uppslag".into(), kind.clone(), q, "--json".into()];
                                if let Ok(j) = Job::start(
                                    "puppa",
                                    &args,
                                    tr!(format!("Searching {}", p.label), format!("Söker {}", p.label)),
                                    ctx,
                                ) {
                                    self.lookup = Some((p.name.clone(), j));
                                }
                            }
                        });
                        if let Some(hits) = self.lookup_hits.get(&p.name).cloned() {
                            let cur = self.values.get(&p.name).cloned().unwrap_or_default();
                            let q = cur.rsplit(',').next().unwrap_or("").trim().to_string();
                            let shown: Vec<crate::users::Pick> =
                                crate::fuzzy::rank(&q, &hits, |h| format!("{} {} {}", h.name, h.value, h.detail))
                                    .into_iter()
                                    .take(8)
                                    .cloned()
                                    .collect();
                            let shown = if shown.is_empty() { hits.iter().take(8).cloned().collect() } else { shown };
                            if hits.is_empty() {
                                ui.label(
                                    RichText::new(tr!(
                                        "No matches (or not signed in).",
                                        "Inga träffar (eller inte inloggad)."
                                    ))
                                    .color(DIM)
                                    .size(12.0),
                                );
                            }
                            for h in shown {
                                let label = if h.detail.is_empty() {
                                    h.name.clone()
                                } else {
                                    format!("{}  ·  {}", h.name, h.detail)
                                };
                                if ui.add(egui::Button::new(RichText::new(label).size(13.0)).frame(false)).clicked() {
                                    let v = self.values.entry(p.name.clone()).or_default();
                                    if many && v.contains(',') {
                                        let mut parts: Vec<String> =
                                            v.split(',').map(|x| x.trim().to_string()).collect();
                                        parts.pop();
                                        parts.push(h.value.clone());
                                        *v = parts.join(",");
                                    } else {
                                        *v = h.value.clone();
                                    }
                                    self.lookup_hits.remove(&p.name);
                                }
                            }
                        }
                    }
                    ui.add_space(4.0);
                }
            }
            if r.writes() {
                ui.add_space(6.0);
                theme::toggle(
                    ui,
                    &mut self.confirm,
                    tr!("I know this changes the tenant", "Jag vet att det här ändrar i tenanten"),
                );
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let can = !busy && (!r.writes() || self.confirm);
                let runs = self.runs_for(r);
                let label = if runs.len() > 1 {
                    tr!(format!("  Run for {} users  ", runs.len()), format!("  Kör för {} användare  ", runs.len()))
                } else {
                    tr!("  Run script  ", "  Kör skript  ").into()
                };
                if theme::primary(ui, can, &label, accent.gamma_multiply(0.85)) {
                    match Job::start_many("puppa", runs, tr!(format!("Run {}", r.name), format!("Kör {}", r.name)), ctx)
                    {
                        Ok(j) => {
                            self.job = Some(j);
                            self.job_open = false;
                            self.check_result = true;
                        }
                        Err(e) => act = Some(Action::Toast(e)),
                    }
                    self.result = None;
                }
                if theme::secondary(ui, true, tr!("Run in terminal", "Kör i terminal"))
                    .on_hover_text(tr!(
                        "for scripts that ask something or need a sign-in",
                        "för skript som frågar något eller behöver en inloggning"
                    ))
                    .clicked()
                {
                    let mut a = self.run_args(r);
                    a.retain(|x| x != "--ja" || self.confirm);
                    act = Some(Action::Terminal(a));
                }
            });
        });
        ui.add_space(10.0);
        job_panel(ui, &self.job, &mut self.job_open);
        if let Some((p, rows)) = &self.result {
            theme::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Resultat · {} rader", rows.len().saturating_sub(1))).size(16.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::secondary(ui, true, tr!("Open the file", "Öppna filen")).clicked() {
                            open_path(p);
                        }
                    });
                });
                egui::ScrollArea::both().id_salt("resultat").max_height(360.0).show(ui, |ui| {
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
                });
            });
        }
        act
    }
}

// ───────────────────────────── Muppa ─────────────────────────────
//
// Formulärets val (mappar, BitLocker, domän, molnfiler, webbläsare, valfria fält och standardvärden) läses ur den
// installerade Muppa (FOLDERS, BITLOCKER, …, DEFAULT_ANS). Listorna nedan används bara om det inte går.

/// Ett val med id, rubrik och förklaring.
#[derive(Clone, Debug, PartialEq)]
pub struct Opt {
    pub id: String,
    pub label: String,
    pub desc: String,
}

fn opt(id: &str, label: &str, desc: &str) -> Opt {
    Opt { id: id.into(), label: label.into(), desc: desc.into() }
}

/// Muppas valmöjligheter.
#[derive(Clone, Debug)]
pub struct MuppaSchema {
    pub folders: Vec<(Opt, bool)>,
    pub bitlocker: Vec<Opt>,
    pub domain: Vec<Opt>,
    pub cloud: Vec<Opt>,
    pub browsers: Vec<Opt>,
    pub optional: Vec<Opt>,
    pub defaults: serde_json::Value,
    /// läst ur den installerade Muppa (annars Ruppas reservlistor)
    pub live: bool,
}

impl Default for MuppaSchema {
    fn default() -> Self {
        let f = |id, l, on| (opt(id, l, ""), on);
        MuppaSchema {
            folders: vec![
                f("skrivbord", tr!("Desktop", "Skrivbord"), true),
                f("dokument", tr!("Documents", "Dokument"), true),
                f("bilder", tr!("Pictures", "Bilder"), true),
                f("hamtade", tr!("Downloads", "Hämtade filer"), true),
                f("musik", tr!("Music", "Musik"), false),
                f("videor", tr!("Videos", "Videor"), false),
                f("favoriter", tr!("Favorites", "Favoriter"), false),
                f("kontakter", tr!("Contacts", "Kontakter"), false),
                f("sticky", "Sticky Notes", false),
                f("signaturer", tr!("Outlook signatures", "Outlook-signaturer"), false),
            ],
            bitlocker: vec![
                opt(
                    "pausa",
                    tr!("Save the key and suspend BitLocker", "Spara nyckeln och pausa BitLocker"),
                    tr!("recommended", "rekommenderas"),
                ),
                opt(
                    "stang-av",
                    tr!("Save the key and turn off BitLocker", "Spara nyckeln och stäng av BitLocker"),
                    tr!("decrypts the whole disk", "dekrypterar hela disken"),
                ),
                opt(
                    "spara",
                    tr!("Only save the key", "Bara spara nyckeln"),
                    tr!("leaves the protection alone", "rör inte skyddet"),
                ),
                opt("ingen", tr!("Leave BitLocker alone", "Rör inte BitLocker"), ""),
            ],
            domain: vec![
                opt("bada", tr!("Leave the domain and join Entra ID", "Lämna domänen och anslut till Entra ID"), ""),
                opt("doman", tr!("Only leave the domain", "Bara lämna domänen"), ""),
                opt(
                    "entra",
                    tr!("Only join Entra ID", "Bara anslut till Entra ID"),
                    tr!("the computer is already out of the domain", "datorn är redan utanför domänen"),
                ),
                opt(
                    "inget",
                    tr!("Neither", "Inget av dem"),
                    tr!("only copying and preparation", "bara kopiering och förberedelser"),
                ),
            ],
            cloud: vec![
                opt(
                    "hoppa",
                    tr!("Skip files that only exist in the cloud", "Hoppa över filer som bara finns i molnet"),
                    tr!("recommended", "rekommenderas"),
                ),
                opt("kopiera", tr!("Try to copy them too", "Försök kopiera dem också"), ""),
            ],
            browsers: vec![opt("edge", "Microsoft Edge", ""), opt("chrome", "Google Chrome", "")],
            optional: vec![
                opt("kund", tr!("Customer", "Kund"), ""),
                opt("ansvarig", tr!("Responsible", "Ansvarig"), ""),
                opt("dator", tr!("Computer name", "Datorns namn"), ""),
                opt("upn", "E-post (UPN) i Entra ID", ""),
                opt("tekniker", tr!("Technician", "Tekniker"), ""),
                opt("arende", tr!("Ticket number", "Ärendenummer"), ""),
            ],
            defaults: serde_json::json!({"malmapp": "C:\\temp\\{användare}", "cloud": "hoppa", "browsers": ["edge"],
                "bitlocker": "pausa", "admin": true, "admin_name": "lokaladmin", "domain": "bada"}),
            live: false,
        }
    }
}

/// Tolka Muppas konstanter (tupler blir listor i JSON).
pub fn parse_muppa_schema(v: &serde_json::Value) -> Option<MuppaSchema> {
    let s = |x: &serde_json::Value, i: usize| x.get(i).and_then(|y| y.as_str()).unwrap_or("").to_string();
    let opts = |k: &str| -> Option<Vec<Opt>> {
        let a = v[k].as_array()?;
        let o: Vec<Opt> = a.iter().map(|x| Opt { id: s(x, 0), label: s(x, 1), desc: s(x, 2) }).collect();
        (!o.is_empty()).then_some(o)
    };
    let folders: Vec<(Opt, bool)> = v["FOLDERS"]
        .as_array()?
        .iter()
        .map(|x| (opt(&s(x, 0), &s(x, 1), ""), x.get(6).and_then(|b| b.as_bool()).unwrap_or(false)))
        .collect();
    Some(MuppaSchema {
        folders,
        bitlocker: opts("BITLOCKER")?,
        domain: opts("DOMAIN")?,
        cloud: opts("CLOUD")?,
        browsers: opts("BROWSERS")?,
        optional: opts("OPTIONAL_FIELDS")?,
        defaults: v["DEFAULT_ANS"].clone(),
        live: true,
    })
}

pub struct MuppaPage {
    schema: MuppaSchema,
    loaded: bool,
    user: String,
    target: String,
    folders: HashSet<String>,
    custom: String,
    cloud: String,
    browsers: HashSet<String>,
    bitlocker: String,
    domain: String,
    admin: bool,
    admin_name: String,
    optional: HashMap<String, String>,
    job: Option<Job>,
    job_open: bool,
    check_result: bool,
    result: Option<(PathBuf, PathBuf, String)>,
}

impl Default for MuppaPage {
    fn default() -> Self {
        let mut m = MuppaPage {
            schema: MuppaSchema::default(),
            loaded: false,
            user: String::new(),
            target: String::new(),
            folders: HashSet::new(),
            custom: String::new(),
            cloud: String::new(),
            browsers: HashSet::new(),
            bitlocker: String::new(),
            domain: String::new(),
            admin: true,
            admin_name: String::new(),
            optional: HashMap::new(),
            job: None,
            job_open: false,
            check_result: false,
            result: None,
        };
        m.apply_defaults();
        m
    }
}

pub fn muppa_out_dir() -> PathBuf {
    std::env::var_os("MUPPA_UT").map(PathBuf::from).unwrap_or_else(|| core::home().join("Documents").join("Muppa"))
}

impl MuppaPage {
    /// Läs om valen ur Muppa (efter en uppdatering).
    pub fn reset(&mut self) {
        self.loaded = false;
    }

    fn apply_defaults(&mut self) {
        let d = self.schema.defaults.clone();
        let st = |k: &str, fb: &str| d[k].as_str().unwrap_or(fb).to_string();
        self.target = st("malmapp", r"C:\temp\{användare}");
        self.cloud = st("cloud", "hoppa");
        self.bitlocker = st("bitlocker", "pausa");
        self.domain = st("domain", "bada");
        self.admin = d["admin"].as_bool().unwrap_or(true);
        self.admin_name = st("admin_name", "lokaladmin");
        self.folders = match d["folders"].as_array() {
            Some(a) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
            None => self.schema.folders.iter().filter(|f| f.1).map(|f| f.0.id.clone()).collect(),
        };
        self.browsers = d["browsers"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_else(|| ["edge".to_string()].into());
    }

    fn load(&mut self) {
        let keys = ["FOLDERS", "BITLOCKER", "DOMAIN", "CLOUD", "BROWSERS", "OPTIONAL_FIELDS", "DEFAULT_ANS"];
        if let Some(s) = core::describe("muppa", &keys).as_ref().and_then(parse_muppa_schema) {
            let first = !self.schema.live;
            self.schema = s;
            if first {
                self.apply_defaults();
            }
        }
        self.loaded = true;
    }

    pub fn args(&self) -> Vec<String> {
        let mut a = vec!["skapa".to_string(), self.user.trim().to_string()];
        let mappar: Vec<&str> =
            self.schema.folders.iter().map(|f| f.0.id.as_str()).filter(|f| self.folders.contains(*f)).collect();
        a.extend(["--mappar".into(), mappar.join(",")]);
        for p in self.custom.split([';', '\n']).map(str::trim).filter(|s| !s.is_empty()) {
            a.extend(["--egen".into(), p.to_string()]);
        }
        let wb: Vec<&str> =
            self.schema.browsers.iter().map(|b| b.id.as_str()).filter(|b| self.browsers.contains(*b)).collect();
        a.extend(["--webblasare".into(), wb.join(",")]);
        a.extend(["--bitlocker".into(), self.bitlocker.clone()]);
        a.extend(["--doman".into(), self.domain.clone()]);
        a.extend(["--molnfiler".into(), self.cloud.clone()]);
        if self.admin && !self.admin_name.trim().is_empty() {
            a.extend(["--admin".into(), self.admin_name.trim().into()]);
        } else {
            a.push("--ingen-admin".into());
        }
        if !self.target.trim().is_empty() {
            a.extend(["--malmapp".into(), self.target.trim().into()]);
        }
        for o in &self.schema.optional {
            if let Some(v) = self.optional.get(&o.id).filter(|v| !v.trim().is_empty()) {
                a.extend([format!("--{}", o.id), v.trim().into()]);
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

    fn choices(ui: &mut Ui, opts: &[Opt], cur: &mut String, accent: Color32) {
        ui.spacing_mut().item_spacing.y = 6.0;
        for o in opts {
            if theme::option_card(ui, *cur == o.id, &o.label, &o.desc, accent) {
                *cur = o.id.clone();
            }
        }
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
            self.result = if ok { self.find_result() } else { None };
            self.job_open = !ok;
        }
        let accent = xterm(170);
        let schema = self.schema.clone();
        egui::ScrollArea::vertical().id_salt("muppa").auto_shrink([false, false]).show(ui, |ui| {
            section_sub(
                ui,
                tr!("Who is moving?", "Vem flyttas?"),
                tr!(
                    "The user whose profile moves from on-premises AD to Entra ID",
                    "Användaren vars profil flyttas från lokal AD till Entra ID"
                ),
                |ui| {
                    egui::Grid::new("vem").num_columns(2).spacing(vec2(14.0, 10.0)).show(ui, |ui| {
                        ui.label(RichText::new(tr!("User", "Användare")).color(DIM));
                        field(
                            ui,
                            &mut self.user,
                            tr!("e.g. anna (the profile folder in C:\\Users)", "t.ex. anna (profilmappen i C:\\Users)"),
                            340.0,
                        );
                        ui.end_row();
                        ui.label(RichText::new(tr!("Target folder", "Målmapp")).color(DIM));
                        field(ui, &mut self.target, "", 340.0);
                        ui.end_row();
                    });
                },
            );
            section_sub(
                ui,
                tr!("Folders that are copied", "Mappar som kopieras"),
                tr!(
                    "Click to choose. Redirected OneDrive folders are followed.",
                    "Klicka för att välja. Omdirigerade OneDrive-mappar följs."
                ),
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                        for (o, _) in &schema.folders {
                            let mut on = self.folders.contains(&o.id);
                            if theme::chip(ui, &mut on, &o.label, accent) {
                                if on {
                                    self.folders.insert(o.id.clone());
                                } else {
                                    self.folders.remove(&o.id);
                                }
                            }
                        }
                    });
                    ui.add_space(6.0);
                    field(
                        ui,
                        &mut self.custom,
                        tr!("Your own paths, separated by ;", "Egna sökvägar, separera med ;"),
                        f32::INFINITY,
                    );
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(tr!(
                            "Files that only exist in the OneDrive cloud",
                            "Filer som bara finns i OneDrive-molnet"
                        ))
                        .color(DIM)
                        .size(12.5),
                    );
                    Self::choices(ui, &schema.cloud, &mut self.cloud, accent);
                },
            );
            ui.columns(2, |cols| {
                section(&mut cols[0], "BitLocker", |ui| {
                    Self::choices(ui, &schema.bitlocker, &mut self.bitlocker, accent)
                });
                section(&mut cols[1], tr!("Domain and Entra ID", "Domän och Entra ID"), |ui| {
                    Self::choices(ui, &schema.domain, &mut self.domain, accent)
                });
            });
            ui.columns(2, |cols| {
                section_sub(
                    &mut cols[0],
                    tr!("Browsers", "Webbläsare"),
                    tr!(
                        "Bookmarks are exported, you save the passwords yourself",
                        "Bokmärken exporteras, lösenord sparar du själv"
                    ),
                    |ui| {
                        for b in &schema.browsers {
                            let mut on = self.browsers.contains(&b.id);
                            if theme::toggle(ui, &mut on, &b.label) {
                                if on {
                                    self.browsers.insert(b.id.clone());
                                } else {
                                    self.browsers.remove(&b.id);
                                }
                            }
                        }
                    },
                );
                section_sub(
                    &mut cols[1],
                    tr!("Local admin account", "Lokalt admin-konto"),
                    tr!("The password is asked for when the script runs", "Lösenordet frågas efter när skriptet körs"),
                    |ui| {
                        theme::toggle(
                            ui,
                            &mut self.admin,
                            tr!("Create a local admin account", "Skapa ett lokalt admin-konto"),
                        );
                        ui.add_enabled_ui(self.admin, |ui| field(ui, &mut self.admin_name, "kontonamn", 220.0));
                    },
                );
            });
            section_sub(ui, tr!("Optional details", "Valfria uppgifter"), "Hamnar i checklistan", |ui| {
                egui::Grid::new("valfritt").num_columns(4).spacing(vec2(14.0, 10.0)).show(ui, |ui| {
                    for (i, o) in schema.optional.iter().enumerate() {
                        ui.label(RichText::new(&o.label).color(DIM));
                        field(ui, self.optional.entry(o.id.clone()).or_default(), "", 220.0);
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            });
            theme::card(ui, |ui| {
                ui.horizontal(|ui| {
                    let ok = !self.user.trim().is_empty() && !busy;
                    if theme::primary(
                        ui,
                        ok,
                        tr!("  Create script and checklist  ", "  Skapa skript och checklista  "),
                        accent.gamma_multiply(0.85),
                    ) {
                        match Job::start(
                            "muppa",
                            &self.args(),
                            tr!(
                                format!("Creating a script for {}", self.user.trim()),
                                format!("Skapar skript för {}", self.user.trim())
                            ),
                            ctx,
                        ) {
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
                        ui.label(RichText::new(tr!("Fill in the user first.", "Fyll i användaren först.")).color(DIM));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::secondary(ui, true, "Guiden i terminal").clicked() {
                            act = Some(Action::Terminal(vec!["ny".into()]));
                        }
                        if theme::secondary(ui, true, tr!("Open the folder", "Öppna mappen")).clicked() {
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
                        if theme::primary(
                            ui,
                            true,
                            tr!("Show the script", "Visa skriptet"),
                            accent.gamma_multiply(0.85),
                        ) {
                            open_path(script);
                        }
                        if theme::secondary(ui, true, tr!("Open the checklist", "Öppna checklistan")).clicked() {
                            open_path(list);
                        }
                        if theme::secondary(ui, true, tr!("Open the folder", "Öppna mappen")).clicked() {
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
        let mut m = MuppaPage { user: "anna".into(), ..Default::default() };
        m.browsers.insert("chrome".into());
        m.optional.insert("kund".into(), "Acme".into());
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

    #[test]
    fn runs_per_user() {
        let one = parse_recipe(
            "anvandare-sparra",
            "<#PUPPA\nnamn: Spärra\ntyp: ändrar\nparam: Anvandare | Användare | | @user\n#>\n",
        )
        .unwrap();
        let many = parse_recipe(
            "grupper-lagg-till",
            "<#PUPPA\nnamn: Lägg till\ntyp: ändrar\nparam: Grupp | Grupp | | @group\nparam: Anvandare | Användare | | @user+\nparam: Roll | Som | Medlem | Medlem,Ägare\n#>\n",
        )
        .unwrap();
        let mut p = PuppaPage { run_for: vec!["a@x.se".into(), "b@x.se".into()], ..Default::default() };
        let runs = p.runs_for(&one);
        assert_eq!(runs.len(), 2, "en körning per användare");
        assert_eq!(runs[0], vec!["kör", "anvandare-sparra", "Anvandare=a@x.se", "--csv", "--ja"]);
        assert_eq!(runs[1][2], "Anvandare=b@x.se");
        p.values.insert("Anvandare".into(), "a@x.se,b@x.se".into());
        p.values.insert("Grupp".into(), "ekonomi@x.se".into());
        p.values.insert("Roll".into(), "Ägare".into());
        let runs = p.runs_for(&many);
        assert_eq!(runs.len(), 1, "@user+ tar alla på en gång");
        assert!(runs[0].contains(&"Anvandare=a@x.se,b@x.se".to_string()));
        assert!(runs[0].contains(&"Roll=Ägare".to_string()));
        assert_eq!(crate::users::user_param(&many), Some(("Anvandare".to_string(), true)));
        assert_eq!(crate::users::user_param(&one), Some(("Anvandare".to_string(), false)));
    }

    #[test]
    fn profiles_and_areas() {
        let v = serde_json::json!({"azure": ["Azure", ["az"]], "bas": ["grund", ["pwsh", "az"]], "ny": ["nytt", []]});
        let p = parse_profiles(&v);
        assert_eq!(p.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), ["bas", "azure", "ny"]);
        assert_eq!(p[0].2, vec!["pwsh", "az"]);
        assert_eq!(profile_label("m365"), "M365");
        assert_eq!(profile_label("bas"), "Base");
        let mut hints = HashMap::new();
        hints.insert("Teams".to_string(), "team, kanaler".to_string());
        assert_eq!(area_info("Teams", &hints).2, "Team, kanaler");
        assert_eq!(area_info("Intune", &hints).1, "IN", "nya områden får en ikon av namnet");
    }

    #[test]
    fn muppa_schema_from_python() {
        // så som describe() levererar Muppas konstanter (tupler blir listor)
        let v = serde_json::json!({
            "FOLDERS": [["skrivbord", "Skrivbord", "shell", "Desktop", "Desktop", "Skrivbord", true],
                        ["nytt", "Ny mapp", "shell", "X", "X", "X", false]],
            "BITLOCKER": [["pausa", "Pausa", "rekommenderas"]],
            "DOMAIN": [["bada", "Båda", ""]],
            "CLOUD": [["hoppa", "Hoppa", ""]],
            "BROWSERS": [["edge", "Microsoft Edge"], ["firefox", "Firefox"]],
            "OPTIONAL_FIELDS": [["kund", "Kund"]],
            "DEFAULT_ANS": {"folders": ["skrivbord"], "browsers": ["edge"], "bitlocker": "pausa"}
        });
        let s = parse_muppa_schema(&v).unwrap();
        assert!(s.live);
        assert_eq!(s.folders.len(), 2);
        assert_eq!(s.folders[1].0.label, "Ny mapp");
        assert!(s.folders[0].1 && !s.folders[1].1);
        assert_eq!(s.browsers[1].id, "firefox");
        assert_eq!(s.bitlocker[0].desc, "rekommenderas");
        assert!(parse_muppa_schema(&serde_json::json!({})).is_none());
    }
}
