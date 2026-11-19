//! En inbyggd terminal: kör en kusin (en TUI) i en riktig pty inne i Ruppas fönster.
//! portable-pty ger pty:n (ConPTY på Windows), vt100 tolkar utdatan, och egui ritar rutnätet.
//! Sessionerna lever vidare när man växlar flik, så man kan hoppa mellan Uppa, Puppa och Muppa.

use crate::core;
use crate::gui::xterm;
use eframe::egui::{self, Color32, Event, FontId, Key, Rect, Sense, Ui, pos2, vec2};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

const BG: Color32 = Color32::from_rgb(0x01, 0x04, 0x09);
const FG: Color32 = Color32::from_rgb(0xe6, 0xed, 0xf3);
/// ANSI 0–15 (som GitHubs mörka terminaltema)
const ANSI16: [Color32; 16] = [
    Color32::from_rgb(0x48, 0x4f, 0x58),
    Color32::from_rgb(0xff, 0x7b, 0x72),
    Color32::from_rgb(0x3f, 0xb9, 0x50),
    Color32::from_rgb(0xd2, 0x99, 0x22),
    Color32::from_rgb(0x58, 0xa6, 0xff),
    Color32::from_rgb(0xbc, 0x8c, 0xff),
    Color32::from_rgb(0x39, 0xc5, 0xcf),
    Color32::from_rgb(0xb1, 0xba, 0xc4),
    Color32::from_rgb(0x6e, 0x76, 0x81),
    Color32::from_rgb(0xff, 0xa1, 0x98),
    Color32::from_rgb(0x56, 0xd3, 0x64),
    Color32::from_rgb(0xe3, 0xb3, 0x41),
    Color32::from_rgb(0x79, 0xc0, 0xff),
    Color32::from_rgb(0xd2, 0xa8, 0xff),
    Color32::from_rgb(0x56, 0xd4, 0xdd),
    Color32::from_rgb(0xff, 0xff, 0xff),
];

fn nz(n: u16) -> NonZeroU16 {
    NonZeroU16::new(n.max(1)).unwrap_or(NonZeroU16::MIN)
}

pub struct Session {
    parser: Arc<Mutex<vt100::Parser>>,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    size: (u16, u16),
    eof: Arc<AtomicBool>,
    pub exit: Option<String>,
    scroll: usize,
}

impl Session {
    /// Starta kusinen i en ny pty. `ctx` ritas om när det kommer ny utdata.
    pub fn spawn(
        name: &'static str,
        args: &[String],
        rows: u16,
        cols: u16,
        ctx: &egui::Context,
    ) -> Result<Self, String> {
        let script = core::cousin_dir(name).join(format!("{name}.py"));
        if !script.exists() {
            return Err(tr!(format!("{name} is not installed"), format!("{name} är inte installerad")));
        }
        let py = core::find_python().ok_or(tr!("found no Python 3.8+", "hittar ingen Python 3.8+"))?;
        let pair = native_pty_system()
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| tr!(format!("could not open a pty: {e}"), format!("kunde inte öppna en pty: {e}")))?;
        let mut cmd = CommandBuilder::new(&py[0]);
        cmd.args(&py[1..]);
        cmd.arg(&script);
        cmd.args(args);
        cmd.cwd(core::home());
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("PYTHONIOENCODING", "utf-8");
        cmd.env("PYTHONUTF8", "1");
        if std::env::var_os("LANG").is_none() && !cfg!(windows) {
            cmd.env("LANG", "en_US.UTF-8");
        }
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| tr!(format!("could not start {name}: {e}"), format!("kunde inte starta {name}: {e}")))?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let parser = Arc::new(Mutex::new(vt100::Parser::new(nz(rows), nz(cols), 5000)));
        let eof = Arc::new(AtomicBool::new(false));
        let (p2, e2, c2) = (parser.clone(), eof.clone(), ctx.clone());
        thread::spawn(move || {
            let mut buf = [0u8; 16 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if let Ok(mut p) = p2.lock() {
                            p.process(&buf[..n]);
                        }
                        c2.request_repaint();
                    }
                }
            }
            e2.store(true, Ordering::Relaxed);
            c2.request_repaint();
        });
        Ok(Session { parser, writer, master: pair.master, child, size: (rows, cols), eof, exit: None, scroll: 0 })
    }

    pub fn running(&mut self) -> bool {
        if self.exit.is_some() {
            return false;
        }
        match self.child.try_wait() {
            Ok(Some(st)) => {
                self.exit = Some(if st.success() {
                    "avslutades".into()
                } else {
                    tr!(
                        format!("exited with code {}", st.exit_code()),
                        format!("avslutades med kod {}", st.exit_code())
                    )
                });
                false
            }
            Ok(None) if self.eof.load(Ordering::Relaxed) => {
                // utdata slut men processen inte riktigt borta än: fråga igen nästa bildruta
                true
            }
            Ok(None) => true,
            Err(e) => {
                self.exit = Some(format!("fel: {e}"));
                false
            }
        }
    }

    pub fn write(&mut self, bytes: &[u8]) {
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
        self.scroll = 0;
    }

    fn resize(&mut self, rows: u16, cols: u16) {
        if (rows, cols) == self.size || rows < 2 || cols < 10 {
            return;
        }
        self.size = (rows, cols);
        let _ = self.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
        if let Ok(mut p) = self.parser.lock() {
            p.set_size(nz(rows), nz(cols));
        }
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.kill();
    }
}

fn color(c: vt100::Color, default: Color32, bold: bool) -> Color32 {
    match c {
        vt100::Color::Default => default,
        vt100::Color::Idx(i) if i < 8 && bold => ANSI16[i as usize + 8],
        vt100::Color::Idx(i) if i < 16 => ANSI16[i as usize],
        vt100::Color::Idx(i) => xterm(i),
        vt100::Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
    }
}

/// Översätt en tangent till de byte en terminal skickar.
pub fn key_bytes(key: Key, m: egui::Modifiers, app_cursor: bool) -> Option<Vec<u8>> {
    let csi = |s: &str| Some(format!("\x1b[{s}").into_bytes());
    let arrow = |c: char| {
        if app_cursor { Some(format!("\x1bO{c}").into_bytes()) } else { Some(format!("\x1b[{c}").into_bytes()) }
    };
    if m.ctrl && !m.alt {
        let name = key.name();
        if name.len() == 1 {
            let ch = name.chars().next().unwrap().to_ascii_lowercase();
            if ch.is_ascii_lowercase() {
                return Some(vec![(ch as u8) & 0x1f]);
            }
        }
    }
    match key {
        Key::Enter => Some(b"\r".to_vec()),
        Key::Backspace => Some(vec![0x7f]),
        Key::Tab if m.shift => csi("Z"),
        Key::Tab => Some(b"\t".to_vec()),
        Key::Escape => Some(b"\x1b".to_vec()),
        Key::ArrowUp => arrow('A'),
        Key::ArrowDown => arrow('B'),
        Key::ArrowRight => arrow('C'),
        Key::ArrowLeft => arrow('D'),
        Key::Home => csi("H"),
        Key::End => csi("F"),
        Key::PageUp => csi("5~"),
        Key::PageDown => csi("6~"),
        Key::Delete => csi("3~"),
        Key::Insert => csi("2~"),
        _ => None,
    }
}

/// Rita terminalen i hela den tillgängliga ytan och skicka tangenttryck till den när den har fokus.
pub fn show(ui: &mut Ui, s: &mut Session, want_focus: bool) {
    let font = FontId::monospace(13.5);
    let (cw, ch) = ui.ctx().fonts_mut(|f| (f.glyph_width(&font, 'M'), f.row_height(&font)));
    let avail = ui.available_size();
    let (rect, resp) = ui.allocate_exact_size(avail, Sense::click());
    let id = resp.id;
    if want_focus || resp.clicked() {
        resp.request_focus();
    }
    let focused = resp.has_focus();
    if focused {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                id,
                egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true },
            )
        });
    }
    let pad = 8.0;
    let cols = (((rect.width() - 2.0 * pad) / cw).floor() as u16).max(10);
    let rows = (((rect.height() - 2.0 * pad) / ch).floor() as u16).max(2);
    s.resize(rows, cols);

    // indata
    if focused {
        let app_cursor = s.parser.lock().map(|p| p.screen().application_cursor()).unwrap_or(false);
        let bracketed = s.parser.lock().map(|p| p.screen().bracketed_paste()).unwrap_or(false);
        let events = ui.input(|i| i.events.clone());
        let mut out: Vec<u8> = Vec::new();
        for e in events {
            match e {
                Event::Text(t) => out.extend_from_slice(t.as_bytes()),
                Event::Paste(t) => {
                    if bracketed {
                        out.extend_from_slice(b"\x1b[200~");
                    }
                    out.extend_from_slice(t.replace("\r\n", "\r").replace('\n', "\r").as_bytes());
                    if bracketed {
                        out.extend_from_slice(b"\x1b[201~");
                    }
                }
                Event::Key { key, pressed: true, modifiers, .. } if !modifiers.mac_cmd => {
                    if let Some(b) = key_bytes(key, modifiers, app_cursor) {
                        out.extend(b);
                    }
                }
                _ => {}
            }
        }
        if !out.is_empty() {
            s.write(&out);
        }
    }
    // rulla bakåt i historiken med mushjulet
    if resp.hovered() {
        let dy = ui.input(|i| i.smooth_scroll_delta.y);
        if dy.abs() > 0.5 {
            let step = (dy / ch).round() as i64;
            s.scroll = (s.scroll as i64 + step).clamp(0, 5000) as usize;
        }
    }

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 8.0, BG);
    let Ok(mut p) = s.parser.lock() else { return };
    p.screen_mut().set_scrollback(s.scroll);
    s.scroll = p.screen().scrollback();
    let screen = p.screen();
    let origin = rect.min + vec2(pad, pad);
    for r in 0..rows {
        for c in 0..cols {
            let Some(cell) = screen.cell(r, c) else { continue };
            if cell.is_wide_continuation() {
                continue;
            }
            let mut fg = color(cell.fgcolor(), FG, cell.bold());
            let mut bg = color(cell.bgcolor(), BG, false);
            if cell.inverse() {
                std::mem::swap(&mut fg, &mut bg);
            }
            if cell.dim() {
                fg = fg.gamma_multiply(0.55);
            }
            let w = if cell.is_wide() { 2.0 * cw } else { cw };
            let x = origin.x + c as f32 * cw;
            let y = origin.y + r as f32 * ch;
            let cr = Rect::from_min_size(pos2(x, y), vec2(w + 0.5, ch + 0.5));
            if bg != BG {
                painter.rect_filled(cr, 0.0, bg);
            }
            let text = cell.contents();
            // blockgrafik ritas som rektanglar: maskoterna blir skarpa och utan glipor
            match text {
                "█" => {
                    painter.rect_filled(cr, 0.0, fg);
                }
                "▀" => {
                    painter.rect_filled(Rect::from_min_size(cr.min, vec2(cr.width(), ch / 2.0 + 0.5)), 0.0, fg);
                }
                "▄" => {
                    painter.rect_filled(
                        Rect::from_min_size(pos2(x, y + ch / 2.0), vec2(cr.width(), ch / 2.0 + 0.5)),
                        0.0,
                        fg,
                    );
                }
                "" | " " => {}
                t => {
                    painter.text(pos2(x, y), egui::Align2::LEFT_TOP, t, font.clone(), fg);
                    if cell.underline() {
                        painter.line_segment([pos2(x, y + ch - 1.5), pos2(x + w, y + ch - 1.5)], (1.0, fg));
                    }
                }
            }
        }
    }
    if !screen.hide_cursor() && s.scroll == 0 {
        let (cr, cc) = screen.cursor_position();
        let r = Rect::from_min_size(origin + vec2(cc as f32 * cw, cr as f32 * ch), vec2(cw, ch));
        if focused {
            painter.rect_filled(r, 1.0, FG.gamma_multiply(0.55));
        } else {
            painter.rect_stroke(r, 1.0, (1.0, FG.gamma_multiply(0.5)), egui::StrokeKind::Inside);
        }
    }
    if s.scroll > 0 {
        painter.text(
            rect.right_top() + vec2(-12.0, 8.0),
            egui::Align2::RIGHT_TOP,
            tr!(format!("↑ {} lines back", s.scroll), format!("↑ {} rader bakåt", s.scroll)),
            FontId::proportional(12.0),
            FG.gamma_multiply(0.6),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        let none = egui::Modifiers::NONE;
        assert_eq!(key_bytes(Key::Enter, none, false), Some(b"\r".to_vec()));
        assert_eq!(key_bytes(Key::ArrowUp, none, false), Some(b"\x1b[A".to_vec()));
        assert_eq!(key_bytes(Key::ArrowUp, none, true), Some(b"\x1bOA".to_vec()));
        assert_eq!(key_bytes(Key::C, egui::Modifiers::CTRL, false), Some(vec![3]));
        assert_eq!(key_bytes(Key::Tab, egui::Modifiers::SHIFT, false), Some(b"\x1b[Z".to_vec()));
        assert_eq!(key_bytes(Key::A, none, false), None, "vanliga bokstäver kommer som Event::Text");
    }

    #[test]
    fn colors() {
        assert_eq!(color(vt100::Color::Idx(1), FG, false), ANSI16[1]);
        assert_eq!(color(vt100::Color::Idx(1), FG, true), ANSI16[9]);
        assert_eq!(color(vt100::Color::Idx(202), FG, false), xterm(202));
        assert_eq!(color(vt100::Color::Default, FG, false), FG);
    }
}
