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

/// En klickbar ruta (områden och skript): färgad ikon, rubrik, upp till två rader text och små etiketter längst ned.
#[allow(clippy::too_many_arguments)]
pub fn tile(
    ui: &mut Ui,
    size: egui::Vec2,
    accent: Color32,
    icon: &str,
    title: &str,
    body: &str,
    badges: &[(String, Color32)],
    selected: bool,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let hover = ui.ctx().animate_bool_responsive(resp.id, resp.hovered() || selected);
    let p = ui.painter();
    let lift = vec2(0.0, -2.0 * hover);
    let r = rect.translate(lift);
    if hover > 0.0 {
        p.add(
            Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_black_alpha((60.0 * hover) as u8) }
                .as_shape(r, 14),
        );
    }
    let fill = SURFACE.lerp_to_gamma(SURFACE2, hover);
    let stroke = if selected { accent } else { BORDER.lerp_to_gamma(accent.gamma_multiply(0.7), hover) };
    p.rect(r, 14.0, fill, Stroke::new(if selected { 1.5 } else { 1.0 }, stroke), StrokeKind::Inside);
    let pad = 16.0;
    let mut y = r.top() + pad;
    let mut x = r.left() + pad;
    if !icon.is_empty() {
        let ir = Rect::from_min_size(pos2(x, y), vec2(38.0, 38.0));
        p.rect_filled(ir, 10.0, accent.gamma_multiply(0.18));
        p.rect_stroke(ir, 10.0, Stroke::new(1.0, accent.gamma_multiply(0.45)), StrokeKind::Inside);
        p.text(ir.center(), Align2::CENTER_CENTER, icon, FontId::proportional(14.0), accent);
        x = ir.right() + 12.0;
    }
    let tw = r.right() - x - pad;
    let tg = p.layout(title.to_string(), FontId::proportional(15.5), TEXT, tw);
    let th = tg.size().y;
    let ty = if icon.is_empty() { y } else { y + (38.0 - th).max(0.0) / 2.0 };
    p.galley(pos2(x, ty), tg, TEXT);
    y = if icon.is_empty() { y + th + 6.0 } else { y + 38.0_f32.max(th) + 10.0 };
    if !body.is_empty() {
        let mut job =
            egui::text::LayoutJob::simple(body.to_string(), FontId::proportional(12.5), MUTED, r.width() - 2.0 * pad);
        job.wrap.max_rows = 2;
        let g = ui.ctx().fonts_mut(|f| f.layout_job(job));
        p.galley(pos2(r.left() + pad, y), g, MUTED);
    }
    // etiketter längst ned
    let mut bx = r.left() + pad;
    let by = r.bottom() - pad - 20.0;
    for (text, col) in badges {
        let g = p.layout_no_wrap(text.clone(), FontId::proportional(11.5), *col);
        let br = Rect::from_min_size(pos2(bx, by), vec2(g.size().x + 16.0, 20.0));
        p.rect(br, 10.0, col.gamma_multiply(0.14), Stroke::new(1.0, col.gamma_multiply(0.4)), StrokeKind::Inside);
        p.galley(pos2(br.left() + 8.0, br.center().y - g.size().y / 2.0), g, *col);
        bx = br.right() + 6.0;
    }
    resp
}

/// Lägg ut rutor i ett rutnät som anpassar antalet kolumner efter bredden.
pub fn grid<T>(ui: &mut Ui, items: &[T], min_w: f32, h: f32, mut each: impl FnMut(&mut Ui, &T, egui::Vec2)) {
    let gap = 12.0;
    let avail = ui.available_width();
    let cols = (((avail + gap) / (min_w + gap)).floor() as usize).max(1);
    let w = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
    for row in items.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for it in row {
                each(ui, it, vec2(w, h));
            }
        });
        ui.add_space(gap - ui.spacing().item_spacing.y + 4.0);
    }
}

/// Segmenterad växel ("Skript | Användare"). True när valet ändrades.
pub fn segmented(ui: &mut Ui, options: &[&str], cur: &mut usize, accent: Color32) -> bool {
    let font = FontId::proportional(14.0);
    let widths: Vec<f32> = options
        .iter()
        .map(|o| ui.painter().layout_no_wrap(o.to_string(), font.clone(), TEXT).size().x + 32.0)
        .collect();
    let (rect, _) = ui.allocate_exact_size(vec2(widths.iter().sum::<f32>() + 8.0, 38.0), Sense::hover());
    ui.painter().rect(rect, 12.0, SURFACE, Stroke::new(1.0, BORDER), StrokeKind::Inside);
    let mut x = rect.left() + 4.0;
    let mut changed = false;
    for (i, (o, w)) in options.iter().zip(&widths).enumerate() {
        let r = Rect::from_min_size(pos2(x, rect.top() + 4.0), vec2(*w, rect.height() - 8.0));
        let resp = ui
            .interact(r, ui.scope_id().with(("seg", options.join("|"), i)), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let on = *cur == i;
        if on {
            ui.painter().rect_filled(r, 9.0, accent.gamma_multiply(0.85));
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 9.0, HOVER);
        }
        ui.painter().text(r.center(), Align2::CENTER_CENTER, *o, font.clone(), if on { Color32::WHITE } else { TEXT });
        if resp.clicked() && !on {
            *cur = i;
            changed = true;
        }
        x += w;
    }
    changed
}
