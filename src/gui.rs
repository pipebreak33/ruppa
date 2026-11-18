//! Fönstret: Ruppa överst med pratbubbla, ett kort per kusin, och en logg med installernas utdata.
//! Allt som tar tid (skanning, installers, self-update) körs i bakgrundstrådar så att fönstret aldrig fryser.

use crate::core::{self, COUSINS, Cousin, RUPPA, Scan, VERSION};
use crate::euppa_page::EuppaPage;
use crate::pages::{Action, MuppaPage, PuppaPage, UppaPage};
use crate::termview::{self, Session};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2,
};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use crate::theme::{self, BG, BORDER, GREEN, MUTED as DIM, RED, SURFACE as CARD, TEXT as FG, YELLOW};

/// xterm-256 → RGB (färgkuben 16–231 och gråskalan 232–255).
pub fn xterm(i: u8) -> Color32 {
    match i {
        16..=231 => {
            let i = i - 16;
            let lv = [0u8, 95, 135, 175, 215, 255];
            Color32::from_rgb(lv[(i / 36) as usize], lv[((i / 6) % 6) as usize], lv[(i % 6) as usize])
        }
        232..=255 => {
            let g = 8 + (i - 232) * 10;
            Color32::from_rgb(g, g, g)
        }
        _ => FG,
    }
}

enum Msg {
    Scanned(Box<Scan>),
    Line(String),
    Done { ok: Vec<Cousin>, failed: Vec<Cousin>, what: &'static str },
    SelfUpdated(Result<Option<String>, String>),
}

#[derive(Clone, Copy, PartialEq)]
pub enum Face {
    Normal,
    Proud,
    Sad,
}

struct Particle {
    p: Pos2,
    v: egui::Vec2,
    color: Color32,
    born: f64,
}

pub struct App {
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    scan: Option<Scan>,
    scanning: bool,
    busy: Option<String>,
    log: Vec<String>,
    show_log: bool,
    say: Option<(String, f64)>,
    face: (Face, f64),
    confetti: Vec<Particle>,
    restart_to: Option<String>,
    seed: u64,
    /// tiden (sekunder) i den senaste bildrutan
    now: f64,
    /// RUPPA_SCREENSHOT=fil.bmp: ta en skärmdump av fönstret när skanningen är klar och stäng (för README och tester)
    shot: Option<String>,
    /// vald flik: None = översikten, Some(kusin) = kusinens inbyggda terminal
    tab: Option<&'static str>,
    /// körande kusiner; de lever vidare när man byter flik
    sessions: HashMap<&'static str, Session>,
    focus_term: bool,
    term_err: Option<(&'static str, String)>,
    /// kusiner som just nu visas som terminal (annars visas deras appsida), med argumenten terminalen startades med
    term_mode: HashMap<&'static str, Vec<String>>,
    /// skämtet i pratbubblan (byts vid varje skanning, inte varje bildruta)
    joke: usize,
    uppa: UppaPage,
    puppa: PuppaPage,
    muppa: MuppaPage,
    euppa: EuppaPage,
    /// väntar på bekräftelse att avinstallera: en kusin, eller "ruppa" = hela Ruppa
    ask_uninstall: Option<&'static str>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = cc.egui_ctx.clone();
        theme::install(&ctx);
        let (tx, rx) = channel();
        let mut app = App {
            tx,
            rx,
            scan: None,
            scanning: false,
            busy: None,
            log: Vec::new(),
            show_log: false,
            say: None,
            face: (Face::Normal, 0.0),
            confetti: Vec::new(),
            restart_to: None,
            seed: 0x9e37_79b9_7f4a_7c15,
            now: 0.0,
            shot: std::env::var("RUPPA_SCREENSHOT").ok().filter(|s| !s.is_empty()),
            tab: std::env::var("RUPPA_TAB").ok().and_then(|t| core::cousin(&t)).map(|c| c.key),
            sessions: HashMap::new(),
            focus_term: true,
            term_err: None,
            term_mode: HashMap::new(),
            joke: 0,
            uppa: UppaPage::default(),
            puppa: PuppaPage::default(),
            muppa: MuppaPage::default(),
            euppa: EuppaPage::default(),
            ask_uninstall: None,
        };
        app.rescan(&ctx);
        app
    }

    fn rnd(&mut self) -> f32 {
        // xorshift: konfettin behöver bara se slumpmässig ut
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed % 10_000) as f32 / 10_000.0
    }

    fn rescan(&mut self, ctx: &egui::Context) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        thread::spawn(move || {
            let s = core::scan();
            let _ = tx.send(Msg::Scanned(Box::new(s)));
            ctx.request_repaint();
        });
    }

    fn run_installers(&mut self, ctx: &egui::Context, targets: Vec<Cousin>, verb: &'static str) {
        if self.busy.is_some() || targets.is_empty() {
            return;
        }
        let what = if verb == "Installerar" { "installerade" } else { "uppdaterade" };
        self.busy = Some(format!("{verb} {}…", names(&targets)));
        self.show_log = true;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        thread::spawn(move || {
            let (mut ok, mut failed) = (Vec::new(), Vec::new());
            for k in targets {
                let _ = tx.send(Msg::Line(format!("── {verb} {} ──", k.name)));
                ctx.request_repaint();
                let c2 = ctx.clone();
                let tx2 = tx.clone();
                let good = core::install_cousin(k.key, &mut |l| {
                    let _ = tx2.send(Msg::Line(core::strip_ansi(&l)));
                    c2.request_repaint();
                });
                if good {
                    ok.push(k)
                } else {
                    let _ = tx.send(Msg::Line(format!("✘ {} strulade", k.name)));
                    failed.push(k)
                }
            }
            let _ = tx.send(Msg::Done { ok, failed, what });
            ctx.request_repaint();
        });
    }

    fn self_update(&mut self, ctx: &egui::Context) {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some("Uppdaterar Ruppa…".into());
        self.show_log = true;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        thread::spawn(move || {
            let tx2 = tx.clone();
            let r = crate::selfupdate::self_update(false, &mut |l| {
                let _ = tx2.send(Msg::Line(l));
            });
            let _ = tx.send(Msg::SelfUpdated(r));
            ctx.request_repaint();
        });
    }

    fn launch(&mut self, k: Cousin) {
        match core::launch_in_terminal(k.key) {
            Ok(()) => self.toast(format!("{} öppnas i ett terminalfönster.", k.name)),
            Err(e) => {
                self.toast(format!("Kunde inte starta {}: {e}", k.name));
                self.face = (Face::Sad, self.now);
            }
        }
    }

    fn toast(&mut self, s: String) {
        self.say = Some((s, self.now));
    }

    fn pump(&mut self, ctx: &egui::Context) {
        let t = ctx.input(|i| i.time);
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Scanned(s) => {
                    self.scan = Some(*s);
                    self.joke = crate::joke_seed();
                    self.scanning = false;
                }
                Msg::Line(l) => {
                    self.log.push(l);
                    if self.log.len() > 5000 {
                        self.log.drain(..1000);
                    }
                }
                Msg::Done { ok, failed, what } => {
                    self.busy = None;
                    // Ruppa är bara ett skal: sidorna läser om allt från de nya versionerna
                    for k in &ok {
                        match k.key {
                            "uppa" => self.uppa.reset(),
                            "puppa" => self.puppa.reset(),
                            "euppa" => self.euppa.reset(),
                            _ => self.muppa.reset(),
                        }
                    }
                    if failed.is_empty() {
                        let what = if ok.len() == 1 { what.strip_suffix('e').unwrap_or(what) } else { what };
                        self.say = Some((format!("Klart! {} är {what}.", names(&ok)), t));
                        self.face = (Face::Proud, t);
                        self.burst(t);
                    } else {
                        self.say = Some((
                            format!(
                                "{} strulade. Loggen nedanför visar varför. Prova igen om en stund.",
                                names(&failed)
                            ),
                            t,
                        ));
                        self.face = (Face::Sad, t);
                    }
                    self.rescan(ctx);
                }
                Msg::SelfUpdated(r) => {
                    self.busy = None;
                    match r {
                        Ok(Some(v)) => {
                            self.say =
                                Some((format!("Nu är jag Ruppa {v}. Starta om så kör du den nya versionen."), t));
                            self.face = (Face::Proud, t);
                            self.restart_to = Some(v);
                            self.burst(t);
                        }
                        Ok(None) => self.say = Some(("Jag är redan senaste versionen.".into(), t)),
                        Err(e) => {
                            self.log.push(format!("✘ {e}"));
                            self.say = Some((format!("Kunde inte uppdatera mig själv: {e}"), t));
                            self.face = (Face::Sad, t);
                        }
                    }
                }
            }
        }
    }

    fn burst(&mut self, t: f64) {
        let colors: Vec<Color32> = crate::term::CONFETTI.iter().map(|&c| xterm(c)).collect();
        for _ in 0..90 {
            let (a, b, c, d) = (self.rnd(), self.rnd(), self.rnd(), self.rnd());
            self.confetti.push(Particle {
                p: pos2(40.0 + a * 120.0, 60.0),
                v: vec2((b - 0.5) * 420.0, -150.0 - c * 320.0),
                color: colors[(d * colors.len() as f32) as usize % colors.len()],
                born: t,
            });
        }
    }

    fn greeting(&self, t: f64) -> String {
        if let Some((s, at)) = &self.say
            && t - at < 8.0
        {
            return s.clone();
        }
        if let Some(b) = &self.busy {
            return format!("{b} Loggen visar vad som händer.");
        }
        match &self.scan {
            None => "Hej! Jag är Ruppa. Jag ringer runt till kusinerna…".into(),
            Some(s) => crate::greeting(s, Some(self.joke)).0,
        }
    }
}

fn names(v: &[Cousin]) -> String {
    core::join_names(&v.iter().map(|c| c.name).collect::<Vec<_>>())
}

// ───────────────────────────── maskoter ─────────────────────────────

/// Rita en maskot (blockgrafik som i terminalen) med levande ögon. `cell` = en terminalcells storlek.
pub fn draw_mascot(p: &egui::Painter, origin: Pos2, cell: egui::Vec2, who: &Cousin, t: f64, face: Face, seed: f64) {
    let sad = face == Face::Sad;
    let bob = ((t * 2.1 + seed).sin() as f32) * cell.y * 0.08;
    let origin = origin + vec2(0.0, bob);
    for (r, row) in who.art.iter().enumerate() {
        let shade = if sad { [181u8, 174, 138, 137, 95, 59][r.min(5)] } else { who.shade[r.min(who.shade.len() - 1)] };
        let col = xterm(shade);
        let row = row.replace("{e}", "     ").replace("{m}", "     ");
        for (c, ch) in row.chars().enumerate() {
            let x = origin.x + c as f32 * cell.x;
            let y = origin.y + r as f32 * cell.y;
            let rect = match ch {
                '█' => Rect::from_min_size(pos2(x, y), cell + vec2(0.5, 0.5)),
                '▄' => Rect::from_min_size(pos2(x, y + cell.y / 2.0), vec2(cell.x + 0.5, cell.y / 2.0 + 0.5)),
                '▀' => Rect::from_min_size(pos2(x, y), vec2(cell.x + 0.5, cell.y / 2.0 + 0.5)),
                _ => continue,
            };
            p.rect_filled(rect, 0.0, col);
        }
    }
    // ögon på kolumn 3 och 5, rad 1; munnen på kolumn 4, rad 2
    let blink = (t + seed) % 4.1 < 0.15;
    let phase = ((t + seed) / 2.3) as i64 % 7;
    let look = match phase {
        2 => -1.0,
        5 => 1.0,
        _ => 0.0,
    };
    for col in [3.0, 5.0] {
        let c = pos2(origin.x + (col + 0.5) * cell.x, origin.y + 1.5 * cell.y);
        let rw = cell.x * 0.42;
        if blink {
            p.line_segment([c - vec2(rw, 0.0), c + vec2(rw, 0.0)], Stroke::new(cell.x * 0.16, Color32::WHITE));
        } else if face == Face::Proud {
            let s = Stroke::new(cell.x * 0.14, Color32::WHITE);
            p.line_segment([c + vec2(-rw, rw * 0.4), c + vec2(0.0, -rw * 0.5)], s);
            p.line_segment([c + vec2(0.0, -rw * 0.5), c + vec2(rw, rw * 0.4)], s);
        } else {
            p.add(Shape::ellipse_filled(c, vec2(rw, cell.y * 0.27), Color32::WHITE));
            p.circle_filled(
                c + vec2(look * rw * 0.45 + rw * 0.1, cell.y * 0.06),
                rw * 0.5,
                Color32::from_rgb(6, 16, 31),
            );
        }
    }
    let m = pos2(origin.x + 4.5 * cell.x, origin.y + 2.35 * cell.y);
    let w = cell.x * 0.55;
    let depth = match face {
        Face::Sad => -cell.y * 0.18,
        Face::Proud => cell.y * 0.3,
        Face::Normal => cell.y * 0.2,
    };
    let pts: Vec<Pos2> = (0..=10)
        .map(|i| {
            let u = i as f32 / 10.0 * 2.0 - 1.0;
            pos2(m.x + u * w, m.y + depth * (1.0 - u * u))
        })
        .collect();
    p.add(Shape::line(pts, Stroke::new(cell.x * 0.18, Color32::WHITE)));
}

fn mascot_size(who: &Cousin, cell: egui::Vec2) -> egui::Vec2 {
    vec2(9.0 * cell.x, who.art.len() as f32 * cell.y)
}

// ───────────────────────────── vyer ─────────────────────────────

fn pill(ui: &mut Ui, text: &str, color: Color32) {
    theme::badge(ui, text, color);
}

impl App {
    fn header(&mut self, ui: &mut Ui, t: f64) {
        let face = if t - self.face.1 < 6.0 { self.face.0 } else { Face::Normal };
        let msg = self.greeting(t);
        let avail = ui.available_width();
        let cell = vec2(11.0, 21.0);
        let ms = mascot_size(&RUPPA, cell);
        let bubble_w = (avail - ms.x - 60.0).clamp(160.0, 560.0);
        let galley = ui.painter().layout(msg, FontId::proportional(17.0), FG, bubble_w - 36.0);
        let bh = (galley.size().y + 32.0).max(56.0);
        let h = ms.y.max(bh) + 16.0;
        let (rect, _) = ui.allocate_exact_size(vec2(avail, h), Sense::hover());
        let p = ui.painter_at(rect.expand(40.0));
        let mo = pos2(rect.left() + 8.0, rect.center().y - ms.y / 2.0);
        draw_mascot(&p, mo, cell, &RUPPA, t, face, 0.0);
        let bx = mo.x + ms.x + 26.0;
        let brect = Rect::from_min_size(pos2(bx, rect.center().y - bh / 2.0), vec2(bubble_w, bh));
        p.rect_filled(brect, 16.0, CARD);
        p.rect_stroke(brect, 16.0, Stroke::new(1.0, BORDER), StrokeKind::Inside);
        let tip = pos2(brect.left(), brect.center().y);
        p.add(Shape::convex_polygon(
            vec![tip + vec2(-12.0, 0.0), tip + vec2(1.0, -7.0), tip + vec2(1.0, 7.0)],
            xterm(202),
            Stroke::NONE,
        ));
        p.galley(brect.min + vec2(18.0, 16.0), galley, FG);
    }

    fn toolbar(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let scan = self.scan.clone();
        let idle = self.busy.is_none();
        ui.horizontal_wrapped(|ui| {
            if let Some(s) = &scan {
                let old = s.outdated();
                let missing = s.missing();
                let n = old.len() + usize::from(s.self_outdated());
                if n > 0 && theme::primary(ui, idle, &format!("Uppdatera allt ({n})"), theme::ACCENT) {
                    if !old.is_empty() {
                        self.run_installers(ctx, old, "Uppdaterar");
                    } else {
                        self.self_update(ctx);
                    }
                }
                if !missing.is_empty() {
                    let label = if missing.len() == COUSINS.len() {
                        "Installera alla".to_string()
                    } else {
                        format!("Installera {}", names(&missing))
                    };
                    if theme::primary(ui, idle, &label, theme::ACCENT) {
                        self.run_installers(ctx, missing, "Installerar");
                    }
                }
            }
            if theme::secondary(ui, !self.scanning && idle, "Skanna om").clicked() {
                self.rescan(ctx);
            }
            let self_label = match scan.as_ref().and_then(|s| s.self_latest.clone()) {
                Some(l) if core::newer(&l, VERSION) => format!("Uppdatera Ruppa ({VERSION} → {l})"),
                _ => "Uppdatera Ruppa".into(),
            };
            if theme::secondary(ui, idle, &self_label).clicked() {
                self.self_update(ctx);
            }
            if self.ask_uninstall == Some("ruppa") {
                ui.label(RichText::new("Ta bort Ruppa och alla kusiner? Dina data behålls.").color(YELLOW));
                if theme::primary(ui, idle, "Ja, avinstallera allt", RED.gamma_multiply(0.8)) {
                    self.uninstall_all(ctx);
                }
                if theme::secondary(ui, true, "Avbryt").clicked() {
                    self.ask_uninstall = None;
                }
            } else if theme::ghost(ui, "Avinstallera Ruppa…") && idle {
                self.ask_uninstall = Some("ruppa");
            }
            if let Some(v) = self.restart_to.clone()
                && theme::primary(ui, true, &format!("Starta om som {v}"), theme::ACCENT)
            {
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe).arg("gui").spawn();
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if self.scanning || self.busy.is_some() {
                ui.add(egui::Spinner::new());
            }
        });
    }

    fn card(&mut self, ui: &mut Ui, ctx: &egui::Context, k: Cousin, w: f32, t: f64, i: usize) {
        let st = self.scan.as_ref().map(|s| s.get(k.key));
        let idle = self.busy.is_none();
        let accent = xterm(k.shade[2.min(k.shade.len() - 1)]);
        theme::card(ui, |ui| {
            ui.set_width(w - 38.0);
            ui.horizontal(|ui| {
                let cell = vec2(6.0, 11.5);
                let ms = mascot_size(&k, cell);
                // alla maskotplatser lika höga (6 rader), så att korten blir lika höga
                let slot = vec2(ms.x + 6.0, 6.0 * cell.y + 8.0);
                let (r, _) = ui.allocate_exact_size(slot, Sense::hover());
                let top = r.min + vec2(0.0, (slot.y - ms.y) / 2.0);
                draw_mascot(ui.painter(), top, cell, &k, t, Face::Normal, 1.3 * i as f64 + 0.7);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(k.name).size(21.0).color(accent));
                        match &st {
                            None => pill(ui, "kollar…", DIM),
                            Some(s) if !s.installed() => pill(ui, "saknas", DIM),
                            Some(s) if s.outdated() => pill(ui, "kan uppdateras", YELLOW),
                            Some(_) => pill(ui, "aktuell", GREEN),
                        }
                    });
                    ui.label(RichText::new(k.desc).color(DIM));
                    let ver = match &st {
                        None => String::new(),
                        Some(s) => match (&s.current, &s.latest) {
                            (None, Some(l)) => format!("senaste {l}"),
                            (None, None) => "inte installerad".into(),
                            (Some(c), Some(l)) if s.outdated() => format!("{c} → {l}"),
                            (Some(c), _) => format!("version {c}"),
                        },
                    };
                    ui.label(RichText::new(ver).monospace().size(12.5));
                });
            });
            ui.add_space(4.0);
            if self.ask_uninstall == Some(k.key) {
                ui.label(RichText::new(format!("Ta bort {} ur Ruppa? Dina data behålls.", k.name)).color(YELLOW));
                ui.horizontal(|ui| {
                    if theme::primary(ui, idle, "Ja, avinstallera", RED.gamma_multiply(0.8)) {
                        self.uninstall(ctx, k);
                    }
                    if theme::secondary(ui, true, "Avbryt").clicked() {
                        self.ask_uninstall = None;
                    }
                });
                return;
            }
            ui.horizontal(|ui| match &st {
                Some(s) if s.installed() => {
                    if theme::primary(ui, true, &format!("  Öppna {}  ", k.name), accent.gamma_multiply(0.75)) {
                        self.select(Some(k.key));
                    }
                    let label = if s.outdated() { "Uppdatera" } else { "Installera om" };
                    if theme::secondary(ui, idle, label).clicked() {
                        self.run_installers(ctx, vec![k], "Uppdaterar");
                    }
                    if theme::ghost(ui, "Avinstallera") && idle {
                        self.ask_uninstall = Some(k.key);
                    }
                }
                Some(_) => {
                    let install = theme::primary(ui, idle, "  Installera  ", accent.gamma_multiply(0.75));
                    if install {
                        self.run_installers(ctx, vec![k], "Installerar");
                    }
                }
                None => {}
            });
        });
    }

    fn reset_page(&mut self, key: &str) {
        match key {
            "uppa" => self.uppa.reset(),
            "puppa" => self.puppa.reset(),
            "euppa" => self.euppa.reset(),
            _ => self.muppa.reset(),
        }
    }

    /// Ta bort en kusin ur ~/.ruppa (dess data behålls) och skanna om.
    fn uninstall(&mut self, ctx: &egui::Context, k: Cousin) {
        self.ask_uninstall = None;
        self.sessions.remove(k.key);
        self.term_mode.remove(k.key);
        match core::uninstall_cousin(k.key) {
            Ok(()) => {
                self.log.push(format!("✔ {} avinstallerad ur Ruppa", k.name));
                self.say = Some((
                    format!("{} är avinstallerad. Dina data finns kvar om du installerar igen.", k.name),
                    self.now,
                ));
            }
            Err(e) => {
                self.log.push(format!("✘ {e}"));
                self.say = Some((format!("Det gick inte att avinstallera {}: {e}", k.name), self.now));
                self.face = (Face::Sad, self.now);
            }
        }
        self.reset_page(k.key);
        if self.tab == Some(k.key) {
            self.tab = None;
        }
        self.rescan(ctx);
    }

    /// Hela Ruppa: kör avinstallationsskriptet i bakgrunden och stäng fönstret (skriptet tar bort programmet).
    fn uninstall_all(&mut self, ctx: &egui::Context) {
        self.ask_uninstall = None;
        match core::uninstall_script(true) {
            Ok(mut cmd) => {
                cmd.stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    cmd.creation_flags(0x0800_0000);
                }
                match cmd.spawn() {
                    Ok(_) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(e) => self.say = Some((format!("Kunde inte avinstallera: {e}"), self.now)),
                }
            }
            Err(e) => self.say = Some((format!("Kunde inte avinstallera: {e}"), self.now)),
        }
    }

    fn select(&mut self, tab: Option<&'static str>) {
        self.tab = tab;
        self.focus_term = true;
    }

    /// ⌘1–5 (Ctrl på Windows/Linux) väljer flik, ⌘[ och ⌘] bläddrar.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        let order: Vec<Option<&'static str>> =
            std::iter::once(None).chain(COUSINS.iter().map(|c| Some(c.key))).collect();
        let keys = [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4, egui::Key::Num5];
        let mut pick = None;
        ctx.input_mut(|i| {
            for (n, k) in keys.iter().enumerate() {
                if i.consume_key(egui::Modifiers::COMMAND, *k) {
                    pick = Some(order[n]);
                }
            }
            let cur = order.iter().position(|t| *t == self.tab).unwrap_or(0);
            if i.consume_key(egui::Modifiers::COMMAND, egui::Key::CloseBracket) {
                pick = Some(order[(cur + 1) % order.len()]);
            }
            if i.consume_key(egui::Modifiers::COMMAND, egui::Key::OpenBracket) {
                pick = Some(order[(cur + order.len() - 1) % order.len()]);
            }
        });
        if let Some(t) = pick {
            self.select(t);
        }
    }

    fn sidebar(&mut self, ui: &mut Ui, t: f64) {
        ui.add_space(4.0);
        let mac = cfg!(target_os = "macos");
        let mut items: Vec<(Option<&'static str>, Cousin, String)> = vec![(None, RUPPA, "Översikt".into())];
        for c in COUSINS {
            items.push((Some(c.key), c, c.name.into()));
        }
        let mut clicked = None;
        for (n, (tab, who, label)) in items.iter().enumerate() {
            let selected = self.tab == *tab;
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::click());
            let p = ui.painter();
            let acc = xterm(who.shade[2.min(who.shade.len() - 1)]);
            if selected {
                p.rect_filled(rect, 12.0, acc.gamma_multiply(0.13));
                p.rect_filled(
                    Rect::from_min_size(rect.min + vec2(0.0, 12.0), vec2(3.0, 28.0)),
                    2.0,
                    xterm(who.shade[2.min(who.shade.len() - 1)]),
                );
            } else if resp.hovered() {
                p.rect_filled(rect, 12.0, theme::HOVER.gamma_multiply(0.7));
            }
            let cell = vec2(3.2, 6.2);
            let ms = mascot_size(who, cell);
            draw_mascot(
                p,
                pos2(rect.left() + 14.0, rect.center().y - ms.y / 2.0),
                cell,
                who,
                t,
                Face::Normal,
                n as f64 * 1.7,
            );
            let col = if selected { FG } else { FG.gamma_multiply(0.8) };
            p.text(
                pos2(rect.left() + 52.0, rect.center().y - 8.0),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(15.0),
                col,
            );
            // status: kör (grön), kan uppdateras (gul), saknas (grå)
            let sub = match tab {
                None => "alla verktyg".into(),
                Some(k) if self.sessions.contains_key(k) => "kör".into(),
                Some(k) => match self.scan.as_ref().map(|s| s.get(k)) {
                    Some(st) if !st.installed() => "saknas".into(),
                    Some(st) if st.outdated() => "nyare finns".into(),
                    Some(_) => "redo".into(),
                    None => String::new(),
                },
            };
            let sub_col = match sub.as_str() {
                "kör" => GREEN,
                "nyare finns" => YELLOW,
                _ => DIM,
            };
            p.text(
                pos2(rect.left() + 52.0, rect.center().y + 10.0),
                Align2::LEFT_CENTER,
                &sub,
                FontId::proportional(11.5),
                sub_col,
            );
            let hint = format!("{}{}", if mac { "⌘" } else { "Ctrl+" }, n + 1);
            p.text(
                pos2(rect.right() - 10.0, rect.center().y),
                Align2::RIGHT_CENTER,
                hint,
                FontId::proportional(11.0),
                DIM,
            );
            if resp.clicked() {
                clicked = Some(*tab);
            }
            ui.add_space(2.0);
        }
        if let Some(t) = clicked {
            self.select(t);
        }
    }

    fn cousin_view(&mut self, ui: &mut Ui, ctx: &egui::Context, key: &'static str) {
        if self.term_mode.contains_key(key) {
            return self.terminal_view(ui, ctx, key);
        }
        let k = core::cousin(key).unwrap_or(RUPPA);
        let installed = self.scan.as_ref().map(|s| s.get(key).installed());
        let accent = xterm(k.shade[2.min(k.shade.len() - 1)]);
        // sidhuvud: maskot + namn till vänster, version och uppdatering till höger
        let st = self.scan.as_ref().map(|s| s.get(key)).unwrap_or_default();
        let top = ui.cursor().min;
        theme::page_header(ui, &k, self.now, k.desc);
        if st.installed() {
            let w = ui.available_width();
            let mut hr = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(Rect::from_min_size(top, vec2(w, 64.0)))
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
            );
            let busy_me = self.busy.as_deref().is_some_and(|b| b.contains(k.name));
            if busy_me {
                hr.add(egui::Spinner::new().color(accent));
                hr.label(RichText::new(format!("Uppdaterar {}…", k.name)).color(DIM));
            } else {
                let label = if st.outdated() {
                    format!(
                        "Uppdatera {}  {} → {}",
                        k.name,
                        st.current.clone().unwrap_or_default(),
                        st.latest.clone().unwrap_or_default()
                    )
                } else {
                    format!("Uppdatera {}", k.name)
                };
                let clicked = if st.outdated() {
                    theme::primary(&mut hr, self.busy.is_none(), &label, accent.gamma_multiply(0.8))
                } else {
                    theme::secondary(&mut hr, self.busy.is_none(), &label)
                        .on_hover_text(format!(
                            "{} {} är senaste versionen. Hämtar den igen.",
                            k.name,
                            st.current.clone().unwrap_or_default()
                        ))
                        .clicked()
                };
                if clicked {
                    self.run_installers(ctx, vec![k], "Uppdaterar");
                    self.show_log = false;
                }
                if let Some(c) = &st.current {
                    hr.label(RichText::new(format!("version {c}")).color(DIM).size(12.5));
                }
            }
        }
        ui.add_space(14.0);
        match installed {
            None => {
                ui.add(egui::Spinner::new());
                return;
            }
            Some(false) => {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(format!("{} är inte installerad än.", k.name)).size(18.0));
                    ui.add_space(8.0);
                    let idle = self.busy.is_none();
                    let b = egui::Button::new(RichText::new(format!("Installera {}", k.name)).strong())
                        .fill(accent.gamma_multiply(0.6));
                    if ui.add_enabled(idle, b).clicked() {
                        self.run_installers(ctx, vec![k], "Installerar");
                        self.tab = None;
                    }
                });
                return;
            }
            Some(true) => {}
        }
        let act = match key {
            "uppa" => self.uppa.ui(ui, ctx),
            "puppa" => self.puppa.ui(ui, ctx),
            "euppa" => self.euppa.ui(ui, ctx),
            _ => self.muppa.ui(ui, ctx),
        };
        match act {
            Some(Action::Terminal(args)) => {
                // en ny terminal med andra argument ersätter den gamla
                if self.term_mode.get(key) != Some(&args) {
                    self.sessions.remove(key);
                }
                self.term_mode.insert(key, args);
                self.focus_term = true;
                self.term_err = None;
            }
            Some(Action::Toast(m)) => self.say = Some((m, self.now)),
            None => {}
        }
    }

    fn terminal_view(&mut self, ui: &mut Ui, ctx: &egui::Context, key: &'static str) {
        let k = core::cousin(key).unwrap_or(RUPPA);
        let installed = self.scan.as_ref().map(|s| s.get(key).installed());
        let accent = xterm(k.shade[2.min(k.shade.len() - 1)]);
        // rubrikrad
        let mut restart = false;
        let mut stop = false;
        let mut back = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new(k.name).size(20.0).strong().color(accent));
            ui.label(RichText::new(k.desc).color(DIM));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(RichText::new("Tillbaka till appen").strong()).clicked() {
                    back = true;
                }
                if self.sessions.contains_key(key) {
                    if ui.button("Stoppa").clicked() {
                        stop = true;
                    }
                    if ui.button("Starta om").clicked() {
                        restart = true;
                    }
                }
                if installed == Some(true)
                    && ui.button("Eget fönster").on_hover_text("Öppna i ett vanligt terminalfönster").clicked()
                {
                    self.launch(k);
                }
            });
        });
        ui.add_space(6.0);
        if back {
            // terminalen får leva vidare i bakgrunden om den kör något
            self.term_mode.remove(key);
            return;
        }
        if stop || restart {
            if let Some(mut s) = self.sessions.remove(key) {
                s.kill();
            }
            self.focus_term = true;
        }
        if stop {
            self.say = Some((format!("{} stoppad.", k.name), self.now));
        }

        match installed {
            None => {
                ui.add(egui::Spinner::new());
                return;
            }
            Some(false) => {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(format!("{} är inte installerad än.", k.name)).size(18.0));
                    ui.add_space(8.0);
                    let idle = self.busy.is_none();
                    if ui
                        .add_enabled(
                            idle,
                            egui::Button::new(RichText::new(format!("Installera {}", k.name)).strong())
                                .fill(accent.gamma_multiply(0.6)),
                        )
                        .clicked()
                    {
                        self.run_installers(ctx, vec![k], "Installerar");
                        self.tab = None;
                    }
                });
                return;
            }
            Some(true) => {}
        }
        if stop {
            ui.vertical_centered(|ui| {
                ui.add_space(30.0);
                if ui.button(RichText::new(format!("Starta {}", k.name)).strong()).clicked() {
                    self.focus_term = true;
                    self.term_err = None;
                    self.sessions.remove(key);
                }
            });
            return;
        }
        if !self.sessions.contains_key(key) {
            if let Some((ek, e)) = &self.term_err
                && *ek == key
            {
                ui.label(RichText::new(format!("Kunde inte starta {}: {e}", k.name)).color(RED));
                if ui.button("Försök igen").clicked() {
                    self.term_err = None;
                }
                return;
            }
            let args = self.term_mode.get(key).cloned().unwrap_or_default();
            match Session::spawn(key, &args, 30, 100, ctx) {
                Ok(s) => {
                    self.sessions.insert(key, s);
                }
                Err(e) => {
                    self.term_err = Some((key, e));
                    return;
                }
            }
        }
        let want = std::mem::take(&mut self.focus_term);
        let Some(s) = self.sessions.get_mut(key) else { return };
        let running = s.running();
        if !running {
            let msg = format!("{} {}. ", k.name, s.exit.clone().unwrap_or_else(|| "avslutades".into()));
            ui.horizontal(|ui| {
                ui.label(RichText::new(msg).color(DIM));
                if ui.button(RichText::new("Starta igen").strong()).clicked() {
                    restart = true;
                }
            });
        }
        termview::show(ui, s, want && running);
        if restart && !running {
            self.sessions.remove(key);
            self.focus_term = true;
        }
    }

    fn log_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let arrow = if self.show_log { "Dölj" } else { "Visa" };
            if ui.add(egui::Button::new(RichText::new(format!("{arrow} logg")).color(DIM)).frame(false)).clicked() {
                self.show_log = !self.show_log;
            }
            if !self.log.is_empty()
                && self.show_log
                && ui.add(egui::Button::new(RichText::new("töm").color(DIM)).frame(false)).clicked()
            {
                self.log.clear();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("ruppa {VERSION} · Wuppa i Rust")).color(DIM).size(11.5));
            });
        });
        if self.show_log {
            egui::Frame::new()
                .fill(Color32::from_rgb(0x01, 0x04, 0x09))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .auto_shrink([false, false])
                        .max_height(ui.available_height().max(80.0))
                        .show(ui, |ui| {
                            if self.log.is_empty() {
                                ui.label(RichText::new("Här syns installernas utdata.").color(DIM).monospace());
                            }
                            for l in &self.log {
                                let col = if l.starts_with('✘') || l.contains("misslyck") || l.contains("error") {
                                    RED
                                } else if l.starts_with("──") {
                                    xterm(209)
                                } else {
                                    FG
                                };
                                ui.label(RichText::new(l).monospace().size(12.0).color(col));
                            }
                        });
                });
        }
    }

    fn draw_confetti(&mut self, ctx: &egui::Context, t: f64) {
        if self.confetti.is_empty() {
            return;
        }
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("konfetti")));
        self.confetti.retain(|q| t - q.born < 2.6);
        for q in &self.confetti {
            let a = (t - q.born) as f32;
            let pos = q.p + q.v * a + vec2(0.0, 520.0 * a * a);
            let alpha = (1.0 - a / 2.6).clamp(0.0, 1.0);
            p.rect_filled(Rect::from_center_size(pos, vec2(6.0, 4.0)), 1.0, q.color.gamma_multiply(alpha));
        }
        ctx.request_repaint();
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let t = ctx.input(|i| i.time);
        self.now = t;
        self.pump(&ctx);
        self.shortcuts(&ctx);
        // en kusin som avslutats av sig själv: rensa bort den när man inte tittar på den
        let tab = self.tab;
        self.sessions.retain(|k, s| Some(*k) == tab || s.running());

        egui::Panel::left("flikar")
            .resizable(false)
            .exact_size(212.0)
            .frame(
                egui::Frame::new().fill(theme::SIDEBAR).stroke(Stroke::new(1.0, BORDER)).inner_margin(egui::Margin {
                    left: 12,
                    right: 12,
                    top: 18,
                    bottom: 12,
                }),
            )
            .show(ui, |ui| self.sidebar(ui, t));

        if let Some(key) = self.tab {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::same(14)))
                .show(ui, |ui| self.cousin_view(ui, &ctx, key));
            self.after_frame(&ctx, t);
            return;
        }

        egui::Panel::bottom("logg")
            .resizable(self.show_log)
            .default_size(220.0)
            .frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::symmetric(16, 8)))
            .show(ui, |ui| self.log_panel(ui));

        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::same(16))).show(
            ui,
            |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    self.header(ui, t);
                    self.toolbar(ui, &ctx);
                    ui.add_space(10.0);
                    let avail = ui.available_width();
                    // responsivt: alla fyra i rad när det får plats, annars 2×2, smalt under varandra
                    let cols = if avail >= 1250.0 {
                        4
                    } else if avail >= 620.0 {
                        2
                    } else {
                        1
                    };
                    let gap = 12.0;
                    let w = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
                    for (r, chunk) in COUSINS.chunks(cols).enumerate() {
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = gap;
                            for (j, k) in chunk.iter().enumerate() {
                                ui.vertical(|ui| {
                                    ui.set_width(w);
                                    self.card(ui, &ctx, *k, w, t, r * cols + j);
                                });
                            }
                        });
                        ui.add_space(gap - 8.0);
                    }
                });
            },
        );

        self.after_frame(&ctx, t);
    }
}

impl App {
    fn after_frame(&mut self, ctx: &egui::Context, t: f64) {
        self.draw_confetti(ctx, t);
        let ready = self.scan.is_some()
            && self.tab.is_none_or(|k| !self.term_mode.contains_key(k) || self.sessions.contains_key(k));
        let delay = std::env::var("RUPPA_SCREENSHOT_DELAY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(if self.tab.is_some() { 4.0 } else { 2.5 });
        if ready
            && t > delay
            && t > 2.5
            && let Some(path) = self.shot.take()
        {
            let c2 = ctx.clone();
            ctx.request_screenshot(move |img| {
                if let Err(e) = write_bmp(&path, &img) {
                    eprintln!("kunde inte spara {path}: {e}");
                }
                c2.send_viewport_cmd(egui::ViewportCommand::Close);
                c2.request_repaint();
            });
        }
        // levande maskoter: rita om ~10 gånger i sekunden räcker för blink och blickar
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// 24-bitars BMP utan beroenden (sips/Förhandsvisning kan göra om den till PNG).
fn write_bmp(path: &str, img: &egui::ColorImage) -> std::io::Result<()> {
    let [w, h] = img.size;
    let row = (w * 3).div_ceil(4) * 4;
    let size = 54 + row * h;
    let mut b = Vec::with_capacity(size);
    b.extend_from_slice(b"BM");
    b.extend_from_slice(&(size as u32).to_le_bytes());
    b.extend_from_slice(&[0; 4]);
    b.extend_from_slice(&54u32.to_le_bytes());
    b.extend_from_slice(&40u32.to_le_bytes());
    b.extend_from_slice(&(w as i32).to_le_bytes());
    b.extend_from_slice(&(h as i32).to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&24u16.to_le_bytes());
    b.extend_from_slice(&[0; 24]);
    for y in (0..h).rev() {
        let start = b.len();
        for x in 0..w {
            let c = img.pixels[y * w + x];
            b.extend_from_slice(&[c.b(), c.g(), c.r()]);
        }
        b.resize(start + row, 0);
    }
    std::fs::write(path, b)
}

pub fn run() -> i32 {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Ruppa")
            .with_inner_size([1180.0, 800.0])
            .with_min_inner_size([420.0, 480.0])
            .with_icon(std::sync::Arc::new(egui::IconData {
                rgba: include_bytes!("../assets/icon/ruppa-256.rgba").to_vec(),
                width: 256,
                height: 256,
            })),
        ..Default::default()
    };
    match eframe::run_native("Ruppa", opts, Box::new(|cc| Ok(Box::new(App::new(cc))))) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("  kunde inte öppna fönstret: {e}\n  Kör ruppa tui för menyn i terminalen.");
            1
        }
    }
}
