//! Användare i Puppa-sidan: hämta tenantens användare (`puppa uppslag user --alla --json`), fuzzy-sök, kryssa en
//! eller flera och välj sedan vad som ska göras med dem. Åtgärderna är Puppas egna skript som tar en användare
//! (param med @user eller @spuser), så nya skript i Puppa dyker upp här av sig själva.

use crate::gui::xterm;
use crate::pages::{Action, Job, Recipe};
use crate::theme::{self, GREEN, MUTED as DIM, RED, TEXT};
use eframe::egui::{self, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

/// En träff från ett uppslag: värde (t.ex. UPN), namn och detalj.
#[derive(Clone, Debug, PartialEq)]
pub struct Pick {
    pub value: String,
    pub name: String,
    pub detail: String,
}

/// Tolka svaret från `puppa uppslag … --json` (sista raden som börjar med `{`).
pub fn parse_lookup(lines: &[String]) -> Result<(String, Vec<Pick>), String> {
    let line = lines.iter().rev().find(|l| l.trim_start().starts_with('{')).ok_or("inget svar från Puppa")?;
    let v: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
    if !v["ok"].as_bool().unwrap_or(false) {
        return Err(v["error"].as_str().unwrap_or("uppslaget misslyckades").to_string());
    }
    let items = v["items"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|i| Pick {
                    value: i["value"].as_str().unwrap_or("").to_string(),
                    name: i["name"].as_str().unwrap_or("").to_string(),
                    detail: i["detail"].as_str().unwrap_or("").to_string(),
                })
                .filter(|p| !p.value.is_empty())
                .collect()
        })
        .unwrap_or_default();
    Ok((v["tenant"].as_str().unwrap_or("").to_string(), items))
}

/// Den parameter i ett skript som tar användare: (namn, flera på en gång?).
pub fn user_param(r: &Recipe) -> Option<(String, bool)> {
    r.params.iter().find_map(|p| {
        let base = p.lookup.trim_start_matches('@');
        let many = base.ends_with('+');
        matches!(base.trim_end_matches('+'), "user" | "spuser").then(|| (p.name.clone(), many))
    })
}

fn initials(name: &str) -> String {
    let mut it = name.split_whitespace().filter_map(|w| w.chars().next());
    let a = it.next().unwrap_or('?');
    let b = it.next_back();
    format!("{}{}", a, b.map(String::from).unwrap_or_default()).to_uppercase()
}

fn avatar_color(s: &str) -> Color32 {
    const PALETTE: [u8; 8] = [37, 75, 33, 177, 49, 141, 202, 71];
    let h = s.bytes().fold(7usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize));
    xterm(PALETTE[h % PALETTE.len()])
}

#[derive(Default)]
pub struct UsersView {
    items: Vec<Pick>,
    tenant: String,
    job: Option<Job>,
    loaded: bool,
    error: Option<String>,
    query: String,
    /// valda användares UPN, i den ordning de valdes
    pub chosen: Vec<String>,
}

pub enum UsersAction {
    /// kör skriptet med id för de valda användarna
    Run(String),
    Page(Action),
}

impl UsersView {
    pub fn reset(&mut self) {
        self.loaded = false;
        self.job = None;
        self.error = None;
    }

    fn fetch(&mut self, ctx: &egui::Context) {
        let args = vec!["uppslag".into(), "user".into(), "--alla".into(), "--json".into()];
        match Job::start("puppa", &args, "Hämtar användarna", ctx) {
            Ok(j) => {
                self.job = Some(j);
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
        self.loaded = true;
    }

    pub fn name_of(&self, upn: &str) -> String {
        self.items.iter().find(|p| p.value == upn).map(|p| p.name.clone()).unwrap_or_else(|| upn.to_string())
    }

    pub fn ui(&mut self, ui: &mut Ui, ctx: &egui::Context, recipes: &[Recipe], accent: Color32) -> Option<UsersAction> {
        if !self.loaded {
            self.fetch(ctx);
        }
        if let Some(j) = &self.job
            && let Some(_ok) = j.finished()
        {
            match parse_lookup(&j.text()) {
                Ok((t, items)) => {
                    self.tenant = t;
                    self.items = items;
                    self.error = None;
                }
                Err(e) => self.error = Some(e),
            }
            self.job = None;
        }
        let mut act = None;

        // laddar / fel
        if self.job.is_some() {
            theme::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().color(accent));
                    ui.label(RichText::new("Hämtar användarna i tenanten …").size(16.0));
                });
                ui.label(
                    RichText::new("Första gången kan Puppa behöva logga in i Microsoft Graph. Det sker i webbläsaren.")
                        .color(DIM),
                );
            });
            return None;
        }
        if let Some(e) = self.error.clone() {
            theme::card(ui, |ui| {
                ui.label(RichText::new("Kunde inte hämta användarna").size(18.0).color(RED));
                ui.label(RichText::new(e).color(DIM));
                ui.label(
                    RichText::new("Oftast behöver Puppa bara logga in i Graph. Gör det i terminalen och försök igen.")
                        .color(DIM),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if theme::primary(ui, true, "  Logga in i terminal  ", accent.gamma_multiply(0.85)) {
                        act = Some(UsersAction::Page(Action::Terminal(vec![
                            "uppslag".into(),
                            "user".into(),
                            "--alla".into(),
                        ])));
                    }
                    if theme::secondary(ui, true, "Försök igen").clicked() {
                        self.reset();
                    }
                });
            });
            return act;
        }

        let avail = ui.available_width();
        let side = avail > 860.0;
        let list_w = if side { avail * 0.55 } else { avail };
        ui.horizontal_top(|ui| {
            // vänster: sök + lista
            ui.vertical(|ui| {
                ui.set_width(list_w);
                theme::card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{} användare", self.items.len())).size(17.0));
                        if !self.tenant.is_empty() {
                            theme::badge(ui, &self.tenant, accent);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if theme::ghost(ui, "Hämta igen") {
                                self.reset();
                            }
                        });
                    });
                    ui.add_space(4.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .hint_text("Sök namn, e-post, titel eller avdelning …")
                            .desired_width(f32::INFINITY)
                            .margin(vec2(10.0, 8.0)),
                    );
                    let hits: Vec<Pick> = crate::fuzzy::rank(&self.query, &self.items, |p| {
                        format!("{} {} {}", p.name, p.value, p.detail)
                    })
                    .into_iter()
                    .cloned()
                    .collect();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{} visas", hits.len())).color(DIM).size(12.5));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !self.chosen.is_empty() && theme::ghost(ui, "Rensa valen") {
                                self.chosen.clear();
                            }
                            if theme::ghost(ui, "Markera alla som visas") {
                                for p in &hits {
                                    if !self.chosen.contains(&p.value) {
                                        self.chosen.push(p.value.clone());
                                    }
                                }
                            }
                        });
                    });
                    ui.add_space(4.0);
                    let row_h = 46.0;
                    egui::ScrollArea::vertical()
                        .id_salt("anvandare")
                        .max_height(ui.available_height().max(240.0))
                        .show_rows(ui, row_h, hits.len(), |ui, range| {
                            for p in &hits[range] {
                                let on = self.chosen.contains(&p.value);
                                if user_row(ui, p, on, accent, row_h) {
                                    if on {
                                        self.chosen.retain(|v| v != &p.value);
                                    } else {
                                        self.chosen.push(p.value.clone());
                                    }
                                }
                            }
                        });
                });
            });
            // höger: valda + åtgärder
            let right = |ui: &mut Ui, me: &mut Self| -> Option<UsersAction> {
                let mut a = None;
                egui::ScrollArea::vertical().id_salt("anv-atgarder").auto_shrink([false, false]).show(ui, |ui| {
                    a = me.actions_ui(ui, recipes, accent);
                });
                a
            };
            if side {
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    ui.set_width(avail - list_w - 14.0);
                    if let Some(a) = right(ui, self) {
                        act = Some(a);
                    }
                });
            }
        });
        if !side && let Some(a) = self.actions_ui(ui, recipes, accent) {
            act = Some(a);
        }
        act
    }

    fn actions_ui(&mut self, ui: &mut Ui, recipes: &[Recipe], accent: Color32) -> Option<UsersAction> {
        let mut act = None;
        theme::card(ui, |ui| {
            if self.chosen.is_empty() {
                ui.label(RichText::new("Välj användare").size(18.0));
                ui.label(
                    RichText::new("Klicka på en eller flera i listan. Sedan visas vad du kan göra med dem.").color(DIM),
                );
                return;
            }
            let n = self.chosen.len();
            ui.label(RichText::new(if n == 1 { "1 vald".to_string() } else { format!("{n} valda") }).size(18.0));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                let mut remove = None;
                for upn in &self.chosen {
                    let mut on = true;
                    if theme::chip(ui, &mut on, &self.name_of(upn), accent) {
                        remove = Some(upn.clone());
                    }
                }
                if let Some(r) = remove {
                    self.chosen.retain(|v| v != &r);
                }
            });
            ui.label(RichText::new("Klicka på en person för att ta bort den ur urvalet.").color(DIM).size(12.0));
        });
        if self.chosen.is_empty() {
            return None;
        }
        ui.add_space(10.0);
        let mut acts: Vec<(&Recipe, bool)> =
            recipes.iter().filter_map(|r| user_param(r).map(|(_, many)| (r, many))).collect();
        acts.sort_by_key(|(r, _)| (r.writes(), r.category.clone(), r.name.clone()));
        for (title, writes) in [("Visa", false), ("Ändra", true)] {
            let group: Vec<&(&Recipe, bool)> = acts.iter().filter(|(r, _)| r.writes() == writes).collect();
            if group.is_empty() {
                continue;
            }
            theme::overline(ui, &format!("{title} · {}", group.len()));
            ui.spacing_mut().item_spacing.y = 6.0;
            for (r, many) in group {
                let (col, icon, _) = crate::pages::area_info(&r.category, &Default::default());
                let note = if *many || self.chosen.len() == 1 {
                    r.category.clone()
                } else {
                    format!("{} · körs en gång per person", r.category)
                };
                if action_row(ui, col, &icon, &r.name, &note, r.writes()) {
                    act = Some(UsersAction::Run(r.id.clone()));
                }
            }
        }
        act
    }
}

fn user_row(ui: &mut Ui, p: &Pick, on: bool, accent: Color32, h: f32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h - 4.0), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let painter = ui.painter();
    if on {
        painter.rect_filled(rect, 10.0, accent.gamma_multiply(0.14));
    } else if resp.hovered() {
        painter.rect_filled(rect, 10.0, theme::HOVER);
    }
    // kryssruta
    let cb = Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(18.0, 18.0));
    if on {
        painter.rect_filled(cb, 5.0, accent);
        let c = cb.center();
        painter.line_segment([c + vec2(-4.5, 0.0), c + vec2(-1.0, 3.5)], Stroke::new(2.0, Color32::WHITE));
        painter.line_segment([c + vec2(-1.0, 3.5), c + vec2(5.0, -4.0)], Stroke::new(2.0, Color32::WHITE));
    } else {
        painter.rect_stroke(cb, 5.0, Stroke::new(1.5, DIM), StrokeKind::Inside);
    }
    // avatar med initialer
    let ac = pos2(rect.left() + 52.0, rect.center().y);
    let col = avatar_color(&p.value);
    painter.circle_filled(ac, 15.0, col.gamma_multiply(0.25));
    painter.text(ac, Align2::CENTER_CENTER, initials(&p.name), FontId::proportional(12.0), col);
    let x = rect.left() + 78.0;
    let w = rect.right() - x - 10.0;
    let name = painter.layout_no_wrap(p.name.clone(), FontId::proportional(14.5), TEXT);
    painter.galley(pos2(x, rect.center().y - 16.0), name, TEXT);
    let mut job = egui::text::LayoutJob::simple(
        if p.detail.is_empty() { p.value.clone() } else { p.detail.clone() },
        FontId::proportional(12.0),
        DIM,
        w,
    );
    job.wrap.max_rows = 1;
    let g = ui.ctx().fonts_mut(|f| f.layout_job(job));
    let gone = p.detail.contains("avstängd");
    painter.galley(pos2(x, rect.center().y + 3.0), g, if gone { RED } else { DIM });
    resp.clicked()
}

fn action_row(ui: &mut Ui, col: Color32, icon: &str, title: &str, note: &str, writes: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let painter = ui.painter();
    let fill = if resp.hovered() { theme::SURFACE2 } else { theme::SURFACE };
    let stroke = if resp.hovered() { col.gamma_multiply(0.7) } else { theme::BORDER };
    painter.rect(rect, 12.0, fill, Stroke::new(1.0, stroke), StrokeKind::Inside);
    let ir = Rect::from_min_size(pos2(rect.left() + 10.0, rect.center().y - 16.0), vec2(32.0, 32.0));
    painter.rect_filled(ir, 9.0, col.gamma_multiply(0.18));
    painter.text(ir.center(), Align2::CENTER_CENTER, icon, FontId::proportional(12.5), col);
    painter.text(
        pos2(ir.right() + 12.0, rect.center().y - 9.0),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(14.5),
        TEXT,
    );
    painter.text(
        pos2(ir.right() + 12.0, rect.center().y + 10.0),
        Align2::LEFT_CENTER,
        note,
        FontId::proportional(11.5),
        DIM,
    );
    let (t, c) = if writes { ("ändrar", theme::WRITE) } else { ("läser", GREEN) };
    painter.text(pos2(rect.right() - 14.0, rect.center().y), Align2::RIGHT_CENTER, t, FontId::proportional(12.0), c);
    resp.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_json() {
        let lines = vec![
            "  (loggar in…)".to_string(),
            r#"{"ok": true, "tenant": "Contoso", "items": [{"value": "a@x.se", "name": "Anna", "detail": "IT"}]}"#
                .to_string(),
        ];
        let (t, items) = parse_lookup(&lines).unwrap();
        assert_eq!(t, "Contoso");
        assert_eq!(items[0], Pick { value: "a@x.se".into(), name: "Anna".into(), detail: "IT".into() });
        let err = vec![r#"{"ok": false, "error": "kunde inte söka (inloggad?)", "items": []}"#.to_string()];
        assert_eq!(parse_lookup(&err).unwrap_err(), "kunde inte söka (inloggad?)");
        assert!(parse_lookup(&["Avbrutet.".to_string()]).is_err());
        assert_eq!(initials("Anna Maria Svensson"), "AS");
        assert_eq!(initials("bo"), "B");
    }
}
