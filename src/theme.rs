//! Ruppas utseende: färger, typsnitt och några egna komponenter (knappar, chips, vippknappar, kort, listrader)
//! så att alla sidor ser likadana och moderna ut.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Rect, Response,
    RichText, Sense, Shadow, Stroke, StrokeKind, TextStyle, Ui, pos2, vec2,
};
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(0x0b, 0x0d, 0x12);
pub const SIDEBAR: Color32 = Color32::from_rgb(0x08, 0x0a, 0x0e);
pub const SURFACE: Color32 = Color32::from_rgb(0x12, 0x15, 0x1c);
pub const SURFACE2: Color32 = Color32::from_rgb(0x19, 0x1d, 0x27);
pub const HOVER: Color32 = Color32::from_rgb(0x22, 0x27, 0x34);
pub const BORDER: Color32 = Color32::from_rgb(0x23, 0x28, 0x34);
pub const TEXT: Color32 = Color32::from_rgb(0xe8, 0xec, 0xf3);
pub const MUTED: Color32 = Color32::from_rgb(0x8a, 0x93, 0xa6);
pub const ACCENT: Color32 = Color32::from_rgb(0xf2, 0x6b, 0x3a);
pub const GREEN: Color32 = Color32::from_rgb(0x3f, 0xc8, 0x6a);
pub const YELLOW: Color32 = Color32::from_rgb(0xf5, 0xb3, 0x3c);
pub const RED: Color32 = Color32::from_rgb(0xff, 0x6b, 0x6b);
pub const WRITE: Color32 = Color32::from_rgb(0xff, 0x9f, 0x5a);

fn read_first(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|p| std::fs::read(p).ok())
}

/// Systemets eget typsnitt (SF Pro på Mac, Segoe UI på Windows, Inter/Cantarell/Ubuntu på Linux).
/// egui:s inbyggda typsnitt ligger kvar som reserv för tecken som saknas.
fn install_fonts(ctx: &egui::Context) {
    let (ui_paths, mono_paths): (&[&str], &[&str]) = if cfg!(target_os = "macos") {
        (&["/System/Library/Fonts/SFNS.ttf"], &["/System/Library/Fonts/SFNSMono.ttf"])
    } else if cfg!(windows) {
        (&[r"C:\Windows\Fonts\segoeui.ttf"], &[r"C:\Windows\Fonts\CascadiaMono.ttf", r"C:\Windows\Fonts\consola.ttf"])
    } else {
        (
            &[
                "/usr/share/fonts/opentype/inter/Inter-Regular.otf",
                "/usr/share/fonts/truetype/inter/Inter-Regular.ttf",
                "/usr/share/fonts/cantarell/Cantarell-VF.otf",
                "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf",
                "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            ],
            &[
                "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Regular.ttf",
                "/usr/share/fonts/truetype/ubuntu/UbuntuMono-R.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            ],
        )
    };
    let mut defs = FontDefinitions::default();
    if let Some(b) = read_first(ui_paths) {
        defs.font_data.insert("system-ui".into(), Arc::new(FontData::from_owned(b)));
        defs.families.entry(FontFamily::Proportional).or_default().insert(0, "system-ui".into());
    }
    if let Some(b) = read_first(mono_paths) {
        defs.font_data.insert("system-mono".into(), Arc::new(FontData::from_owned(b)));
        defs.families.entry(FontFamily::Monospace).or_default().insert(0, "system-mono".into());
    }
    ctx.set_fonts(defs);
}

pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.all_styles_mut(|s| {
        s.visuals = egui::Visuals::dark();
        let v = &mut s.visuals;
        v.panel_fill = BG;
        v.window_fill = SURFACE;
        v.extreme_bg_color = Color32::from_rgb(0x0e, 0x11, 0x17);
        v.faint_bg_color = Color32::from_rgb(0x15, 0x19, 0x22);
        v.override_text_color = Some(TEXT);
        v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
        v.selection.stroke = Stroke::new(1.0, ACCENT);
        v.hyperlink_color = ACCENT;
        v.window_corner_radius = CornerRadius::same(14);
        v.menu_corner_radius = CornerRadius::same(10);
        v.window_stroke = Stroke::new(1.0, BORDER);
        v.popup_shadow = Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(120) };
        let r = CornerRadius::same(10);
        let w = &mut v.widgets;
        w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        w.noninteractive.corner_radius = r;
        for (st, fill, stroke) in [
            (&mut w.inactive, SURFACE2, BORDER),
            (&mut w.hovered, HOVER, Color32::from_rgb(0x3a, 0x42, 0x56)),
            (&mut w.active, Color32::from_rgb(0x2a, 0x31, 0x42), Color32::from_rgb(0x4a, 0x53, 0x6a)),
            (&mut w.open, HOVER, Color32::from_rgb(0x3a, 0x42, 0x56)),
        ] {
            st.bg_fill = fill;
            st.weak_bg_fill = fill;
            st.bg_stroke = Stroke::new(1.0, stroke);
            st.fg_stroke = Stroke::new(1.5, TEXT);
            st.corner_radius = r;
            st.expansion = 0.0;
        }
        s.spacing.button_padding = vec2(14.0, 7.0);
        s.spacing.interact_size = vec2(40.0, 32.0);
        s.spacing.item_spacing = vec2(10.0, 10.0);
        s.spacing.combo_height = 320.0;
        s.text_styles = [
            (TextStyle::Heading, FontId::proportional(26.0)),
            (TextStyle::Body, FontId::proportional(14.5)),
            (TextStyle::Button, FontId::proportional(14.5)),
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
    });
}

/// Fylld knapp i accentfärg (eller kusinens färg).
pub fn primary(ui: &mut Ui, enabled: bool, text: &str, color: Color32) -> bool {
    let b = egui::Button::new(RichText::new(text).color(Color32::WHITE).size(14.5))
        .fill(color)
        .stroke(Stroke::NONE)
        .corner_radius(10)
        .min_size(vec2(0.0, 36.0));
    ui.add_enabled(enabled, b).clicked()
}

/// Dämpad knapp med ram.
pub fn secondary(ui: &mut Ui, enabled: bool, text: &str) -> Response {
    let b = egui::Button::new(RichText::new(text).size(14.0)).corner_radius(10).min_size(vec2(0.0, 36.0));
    ui.add_enabled(enabled, b)
}

/// Textknapp utan ram.
pub fn ghost(ui: &mut Ui, text: &str) -> bool {
    ui.add(egui::Button::new(RichText::new(text).color(MUTED)).frame(false)).clicked()
}

/// Ett chip som kan slås av och på (för t.ex. mappar som ska kopieras).
pub fn chip(ui: &mut Ui, on: &mut bool, text: &str, color: Color32) -> bool {
    let font = FontId::proportional(13.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, TEXT);
    let check = if *on { 18.0 } else { 0.0 };
    let size = vec2(galley.size().x + 28.0 + check, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    let (fill, stroke) = if *on {
        (color.gamma_multiply(0.22), color.gamma_multiply(0.8))
    } else if resp.hovered() {
        (HOVER, Color32::from_rgb(0x3a, 0x42, 0x56))
    } else {
        (SURFACE2, BORDER)
    };
    p.rect(rect, 16.0, fill, Stroke::new(1.0, stroke), StrokeKind::Inside);
    let mut x = rect.left() + 14.0;
    if *on {
        let c = pos2(x + 5.0, rect.center().y);
        p.line_segment([c + vec2(-4.0, 0.0), c + vec2(-1.0, 3.5)], Stroke::new(2.0, color));
        p.line_segment([c + vec2(-1.0, 3.5), c + vec2(5.0, -4.0)], Stroke::new(2.0, color));
        x += check;
    }
    p.galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, TEXT);
    if resp.clicked() {
        *on = !*on;
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Vippknapp (iOS-stil) med etikett.
pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let (rect, resp) = ui.allocate_exact_size(vec2(40.0, 22.0), Sense::click());
        if resp.clicked() {
            *on = !*on;
            changed = true;
        }
        let t = ui.ctx().animate_bool_responsive(resp.id, *on);
        let p = ui.painter();
        let bg = Color32::from_rgb(0x2a, 0x30, 0x3d).lerp_to_gamma(ACCENT, t);
        p.rect_filled(rect, 11.0, bg);
        let cx = egui::lerp(rect.left() + 11.0..=rect.right() - 11.0, t);
        p.circle_filled(pos2(cx, rect.center().y), 8.5, Color32::WHITE);
        let l = ui.add(egui::Label::new(label).sense(Sense::click()));
        if l.clicked() {
            *on = !*on;
            changed = true;
        }
    });
    changed
}

/// Ett valkort (ersätter radioknappar): titel, beskrivning, markerat med färgad ram.
pub fn option_card(ui: &mut Ui, selected: bool, title: &str, desc: &str, color: Color32) -> bool {
    let w = ui.available_width();
    let title_g = ui.painter().layout(title.to_string(), FontId::proportional(14.5), TEXT, w - 48.0);
    let desc_g =
        (!desc.is_empty()).then(|| ui.painter().layout(desc.to_string(), FontId::proportional(12.0), MUTED, w - 48.0));
    let h = 20.0 + title_g.size().y + desc_g.as_ref().map(|g| g.size().y + 2.0).unwrap_or(0.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    let p = ui.painter();
    let (fill, stroke) = if selected {
        (color.gamma_multiply(0.12), color.gamma_multiply(0.9))
    } else if resp.hovered() {
        (HOVER, Color32::from_rgb(0x3a, 0x42, 0x56))
    } else {
        (SURFACE2, BORDER)
    };
    p.rect(rect, 10.0, fill, Stroke::new(if selected { 1.5 } else { 1.0 }, stroke), StrokeKind::Inside);
    let dot = pos2(rect.left() + 18.0, rect.top() + 10.0 + title_g.size().y / 2.0);
    p.circle_stroke(dot, 7.0, Stroke::new(1.5, if selected { color } else { MUTED }));
    if selected {
        p.circle_filled(dot, 3.8, color);
    }
    let ty = rect.top() + 10.0;
    let th = title_g.size().y;
    p.galley(pos2(rect.left() + 36.0, ty), title_g, TEXT);
    if let Some(g) = desc_g {
        p.galley(pos2(rect.left() + 36.0, ty + th + 2.0), g, MUTED);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Ett kort: ytfärg, rundade hörn, tunn ram och mjuk skugga.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(16)
        .inner_margin(Margin::same(18))
        .shadow(Shadow { offset: [0, 4], blur: 18, spread: 0, color: Color32::from_black_alpha(70) })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Kort med rubrik och valfri underrubrik.
pub fn card_titled(ui: &mut Ui, title: &str, sub: &str, add: impl FnOnce(&mut Ui)) {
    card(ui, |ui| {
        ui.label(RichText::new(title).size(16.0).color(TEXT));
        if !sub.is_empty() {
            ui.label(RichText::new(sub).size(12.5).color(MUTED));
        }
        ui.add_space(8.0);
        add(ui);
    });
    ui.add_space(12.0);
}

/// En rad i en lista: vänsterställd text, färgad prick, markering för vald rad.
pub fn list_row(ui: &mut Ui, selected: bool, title: &str, dot: Option<Color32>, accent: Color32) -> bool {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 34.0), Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 8.0, accent.gamma_multiply(0.16));
        p.rect_filled(Rect::from_min_size(rect.min + vec2(0.0, 8.0), vec2(3.0, 18.0)), 2.0, accent);
    } else if resp.hovered() {
        p.rect_filled(rect, 8.0, HOVER);
    }
    let mut x = rect.left() + 14.0;
    if let Some(c) = dot {
        p.circle_filled(pos2(x + 3.0, rect.center().y), 3.5, c);
        x += 14.0;
    }
    let font = FontId::proportional(14.0);
    let max = rect.right() - x - 8.0;
    let g = p.layout(title.to_string(), font, if selected { TEXT } else { TEXT.gamma_multiply(0.88) }, max);
    let g = if g.rows.len() > 1 {
        p.layout_no_wrap(crate::term::cut(title, (max / 7.5) as usize), FontId::proportional(14.0), TEXT)
    } else {
        g
    };
    p.galley(pos2(x, rect.center().y - g.size().y / 2.0), g, TEXT);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Liten etikett med färgad bakgrund.
pub fn badge(ui: &mut Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.15))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.45)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(color).size(12.0));
        });
}

/// Liten rubrik över en grupp ("SHAREPOINT · 12").
pub fn overline(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text.to_uppercase()).color(MUTED).size(11.0).extra_letter_spacing(0.8));
    ui.add_space(2.0);
}

/// Sidhuvud med kusinens maskot, namn och undertext.
pub fn page_header(ui: &mut Ui, who: &crate::core::Cousin, t: f64, sub: &str) {
    let h = 64.0;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    let cell = vec2(4.6, 8.6);
    let mh = who.art.len() as f32 * cell.y;
    crate::gui::draw_mascot(
        ui.painter(),
        pos2(rect.left(), rect.center().y - mh / 2.0),
        cell,
        who,
        t,
        crate::gui::Face::Normal,
        0.4,
    );
    let x = rect.left() + 9.0 * cell.x + 18.0;
    let accent = crate::gui::xterm(who.shade[2.min(who.shade.len() - 1)]);
    ui.painter().text(
        pos2(x, rect.center().y - 11.0),
        Align2::LEFT_CENTER,
        who.name,
        FontId::proportional(26.0),
        accent,
    );
    ui.painter().text(pos2(x, rect.center().y + 15.0), Align2::LEFT_CENTER, sub, FontId::proportional(13.5), MUTED);
}
