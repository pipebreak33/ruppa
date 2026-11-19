//! Terminalen: färger, maskoten med pratbubbla, spinner, konfetti och TUI-menyn (när inget fönster går att öppna).

use crate::core::{self, Cousin, RUPPA};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal;
use regex::Regex;
use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

static COLOR: AtomicBool = AtomicBool::new(false);
static ANIM: AtomicBool = AtomicBool::new(false);

pub fn init(no_anim: bool) {
    #[cfg(windows)]
    let _ = crossterm::ansi_support::supports_ansi();
    let tty = std::io::stdout().is_terminal();
    COLOR.store(tty && std::env::var_os("NO_COLOR").is_none(), Ordering::Relaxed);
    ANIM.store(
        tty && !no_anim && std::env::var_os("RUPPA_NO_ANIM").is_none() && std::env::var_os("CI").is_none(),
        Ordering::Relaxed,
    );
}

pub fn color() -> bool {
    COLOR.load(Ordering::Relaxed)
}
pub fn anim() -> bool {
    ANIM.load(Ordering::Relaxed)
}
pub fn tty() -> bool {
    std::io::stdout().is_terminal()
}

pub fn c(code: &str, s: &str) -> String {
    if color() { format!("\x1b[{code}m{s}\x1b[0m") } else { s.to_string() }
}
pub fn c256(n: u8, s: &str) -> String {
    c(&format!("38;5;{n}"), s)
}
pub fn green(s: &str) -> String {
    c256(42, s)
}
pub fn yellow(s: &str) -> String {
    c256(214, s)
}
pub fn red(s: &str) -> String {
    c256(203, s)
}
pub fn dim(s: &str) -> String {
    c("2", s)
}
pub fn bold(s: &str) -> String {
    c("1", s)
}
pub fn accent(s: &str) -> String {
    c256(202, s)
}

fn ansi_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\x1b\[[0-9;?]*[a-zA-Z]").unwrap())
}
pub fn vlen(s: &str) -> usize {
    ansi_re().replace_all(s, "").chars().count()
}
pub fn pad(s: &str, n: usize) -> String {
    format!("{s}{}", " ".repeat(n.saturating_sub(vlen(s))))
}
pub fn cut(s: &str, n: usize) -> String {
    let plain = ansi_re().replace_all(s, "").replace('\t', " ");
    let plain = plain.trim();
    if plain.chars().count() <= n {
        plain.to_string()
    } else {
        format!("{}…", plain.chars().take(n.saturating_sub(1)).collect::<String>())
    }
}
pub fn width() -> usize {
    terminal::size().map(|(w, _)| w as usize).unwrap_or(100).max(20)
}
pub fn height() -> usize {
    terminal::size().map(|(_, h)| h as usize).unwrap_or(30).max(8)
}

/// Klipp en (färgad) rad så att den aldrig radbryts.
pub fn fit(s: &str, w: usize) -> String {
    let w = w.max(8) - 1;
    if vlen(s) <= w {
        return s.to_string();
    }
    let mut out = String::new();
    let mut n = 0;
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < s.len() && n < w - 1 {
        if bytes[i] == 0x1b
            && let Some(m) = ansi_re().find_at(s, i).filter(|m| m.start() == i)
        {
            out.push_str(m.as_str());
            i = m.end();
            continue;
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
        n += 1;
    }
    if color() {
        out.push_str("\x1b[0m");
    }
    out.push('…');
    out
}

/// Ritar om ett block rader på plats (även efter att fönstret ändrat storlek).
#[derive(Default)]
pub struct Live {
    prev: Vec<usize>,
}

impl Live {
    fn up(&self) -> usize {
        let w = width();
        self.prev.iter().map(|n| n.div_ceil(w).max(1)).sum()
    }
    pub fn draw(&mut self, lines: &[String]) {
        let w = width();
        let lines: Vec<String> = lines.iter().map(|l| fit(l, w)).collect();
        let mut out = format!("\r{}\x1b[J", "\x1b[F".repeat(self.up()));
        for l in &lines {
            out.push_str(l);
            out.push_str("\r\n"); // \r: i rått läge flyttar \n inte till radens början
        }
        print!("{out}");
        let _ = std::io::stdout().flush();
        self.prev = lines.iter().map(|l| vlen(l)).collect();
    }
    pub fn clear(&mut self) {
        print!("\r{}\x1b[J", "\x1b[F".repeat(self.up()));
        let _ = std::io::stdout().flush();
        self.prev.clear();
    }
}

// ───────────────────────────── maskoten ─────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mood {
    Happy,
    Blink,
    Left,
    Right,
    Sad,
    Proud,
    Wow,
    Worry,
}

pub fn eyes(m: Mood) -> &'static str {
    match m {
        Mood::Blink => " ─ ─ ",
        Mood::Left => "◕ ◕  ",
        Mood::Right => "  ◕ ◕",
        Mood::Proud => " ^ ^ ",
        Mood::Worry => " ◔ ◔ ",
        _ => " ◕ ◕ ",
    }
}
pub fn mouth(m: Mood) -> &'static str {
    match m {
        Mood::Left => " ‿   ",
        Mood::Right => "   ‿ ",
        Mood::Sad => "  ⁀  ",
        Mood::Proud => "  ◡  ",
        Mood::Wow => "  o  ",
        Mood::Worry => "  ~  ",
        _ => "  ‿  ",
    }
}

const SAD: [u8; 6] = [181, 174, 138, 137, 95, 59];

pub fn art(who: &Cousin, mood: Mood) -> Vec<String> {
    who.art
        .iter()
        .enumerate()
        .map(|(k, row)| {
            let sh = if mood == Mood::Sad { SAD[k.min(5)] } else { who.shade[k.min(who.shade.len() - 1)] };
            if let Some((a, b)) = row.split_once("{e}") {
                format!("{}{}{}", c256(sh, a), bold(eyes(mood)), c256(sh, b))
            } else if let Some((a, b)) = row.split_once("{m}") {
                format!("{}{}{}", c256(sh, a), bold(mouth(mood)), c256(sh, b))
            } else {
                c256(sh, row)
            }
        })
        .collect()
}

pub fn wrap(msg: &str, w: usize, maxl: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in ansi_re().replace_all(msg, "").split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > w {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        } else {
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(word);
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.len() > maxl {
        lines.truncate(maxl);
        let last = lines.pop().unwrap();
        lines.push(cut(&format!("{last} …"), w));
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines.into_iter().map(|l| cut(&l, w)).collect()
}

/// Maskot + pratbubbla. Under 44 kolumner döljs maskoten.
pub fn frame(msg: &str, mood: Mood, lift: usize, top: &str, sub: &str) -> Vec<String> {
    let w = width();
    let logo = accent(&bold(RUPPA.logo));
    if w < 44 {
        let mut out = Vec::new();
        for (k, l) in wrap(msg, w.saturating_sub(6), 4).into_iter().enumerate() {
            out.push(if k == 0 { format!("  {logo} {l}") } else { format!("     {l}") });
        }
        if !sub.is_empty() {
            out.push(format!("     {}", dim(&cut(sub, w.saturating_sub(7)))));
        }
        return out;
    }
    let a = art(&RUPPA, mood);
    let n = a.len() + 2;
    let mut rows = vec![String::new(); n];
    for (k, r) in a.into_iter().enumerate() {
        rows[k + 1 - lift.min(1)] = r;
    }
    if !top.is_empty() && rows[0].is_empty() {
        rows[0] = top.to_string();
    }
    let mut bub = Vec::new();
    if !msg.is_empty() {
        let lines = wrap(msg, (w.saturating_sub(20)).clamp(14, 60), 4);
        let iw = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        bub.push(dim(&format!("╭{}╮", "─".repeat(iw + 2))));
        for l in &lines {
            bub.push(format!("{}{l}{}{}", dim("│ "), " ".repeat(iw - l.chars().count()), dim(" │")));
        }
        bub.push(dim(&format!("╰{}╯", "─".repeat(iw + 2))));
    }
    let mut res = Vec::new();
    for (k, row) in rows.iter().enumerate() {
        let mut line = format!("  {}", pad(row, 10));
        if k >= 1 && k - 1 < bub.len() {
            line.push(' ');
            line.push_str(&if k == 2 { accent("◂ ") } else { "  ".into() });
            line.push_str(&bub[k - 1]);
        } else if !sub.is_empty() && k >= 1 && k - 1 == bub.len() {
            line.push_str(&format!("    {}", dim(&cut(sub, w.saturating_sub(19)))));
        }
        res.push(fit(&line, w));
    }
    while res.last().is_some_and(|l| ansi_re().replace_all(l, "").trim().is_empty()) {
        res.pop();
    }
    while res.first().is_some_and(|l| ansi_re().replace_all(l, "").trim().is_empty()) {
        res.remove(0);
    }
    res
}

pub fn say(msg: &str, mood: Mood) {
    if tty() {
        println!("{}", frame(msg, mood, 0, "", "").join("\n"));
    } else {
        println!("  {} {msg}", RUPPA.logo);
    }
}

/// Blinkar då och då, tittar runt ibland.
pub fn idle_mood(t: f64) -> Mood {
    if t % 4.1 < 0.16 {
        return Mood::Blink;
    }
    match (t / 2.3) as u64 % 7 {
        2 => Mood::Left,
        5 => Mood::Right,
        _ => Mood::Happy,
    }
}

/// Konfetti i släktens färger.
pub const CONFETTI: [u8; 5] = [202, 49, 75, 177, 220];

fn rnd() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(Instant::now().elapsed().as_nanos());
    h.finish()
}

pub fn celebrate(msg: &str) {
    if !anim() {
        return say(msg, Mood::Proud);
    }
    let mut live = Live::default();
    print!("\x1b[?25l");
    for f in 0..14 {
        let top: String = (0..10)
            .map(|_| {
                let r = rnd();
                if r % 100 < 35 {
                    let ch = ["✦", "✧", "⋆", "·", "*"][(r / 100 % 5) as usize];
                    c256(CONFETTI[(r / 1000 % 5) as usize], ch)
                } else {
                    " ".into()
                }
            })
            .collect();
        live.draw(&frame(msg, if f % 5 != 0 { Mood::Proud } else { Mood::Blink }, f % 2, &top, ""));
        thread::sleep(Duration::from_millis(90));
    }
    live.draw(&frame(msg, Mood::Proud, 0, "", ""));
    print!("\x1b[?25h");
    let _ = std::io::stdout().flush();
}

/// Ruppa svävar och blinkar medan något pågår i bakgrunden.
pub fn with_spinner<T: Send + 'static>(msg: &str, work: impl FnOnce() -> T + Send + 'static) -> T {
    if !anim() {
        return work();
    }
    let done = Arc::new(AtomicBool::new(false));
    let d2 = done.clone();
    let msg = msg.to_string();
    let anim_t = thread::spawn(move || {
        let mut live = Live::default();
        print!("\x1b[?25l");
        let t0 = Instant::now();
        while !d2.load(Ordering::Relaxed) {
            let t = t0.elapsed().as_secs_f64();
            live.draw(&frame(&msg, idle_mood(t), (t * 2.5) as usize % 2, "", ""));
            thread::sleep(Duration::from_millis(120));
        }
        live.clear();
        print!("\x1b[?25h");
        let _ = std::io::stdout().flush();
    });
    let out = work();
    done.store(true, Ordering::Relaxed);
    let _ = anim_t.join();
    out
}

// ───────────────────────────── statusrader ─────────────────────────────

pub fn cousin_line(k: &Cousin, s: &core::Status, desc: bool) -> String {
    let w = width();
    let (mark, ver) = if !s.installed() {
        (
            dim("○"),
            dim(&format!(
                "{}{}",
                tr!("missing", "saknas"),
                s.latest
                    .as_ref()
                    .map(|l| tr!(format!(" · {l} available"), format!(" · {l} finns")))
                    .unwrap_or_default()
            )),
        )
    } else if s.outdated() {
        (
            yellow("●"),
            format!(
                "{} {} {}",
                s.current.as_deref().unwrap_or("?"),
                dim("→"),
                yellow(s.latest.as_deref().unwrap_or("?"))
            ),
        )
    } else {
        (green("●"), dim(s.current.as_deref().unwrap_or("?")))
    };
    let head =
        format!("  {mark} {} {} ", c256(k.shade[2.min(k.shade.len() - 1)], &bold(k.logo)), pad(&bold(k.name), 6));
    if w < 58 || !desc {
        return head + &ver;
    }
    format!("{head}{}  {}", pad(&ver, 18), dim(&cut(k.desc(), w.saturating_sub(40).max(10))))
}

pub fn status_lines(scan: &core::Scan, desc: bool) -> Vec<String> {
    let mut v: Vec<String> = core::COUSINS.iter().map(|k| cousin_line(k, &scan.get(k.key), desc)).collect();
    if scan.self_outdated() {
        v.push(format!(
            "  {} {} {} {} {} {}",
            yellow("●"),
            accent(&bold(RUPPA.logo)),
            pad(&bold("Ruppa"), 6),
            core::VERSION,
            dim("→"),
            yellow(scan.self_latest.as_deref().unwrap_or(""))
        ));
    }
    v
}

// ───────────────────────────── TUI-meny ─────────────────────────────

pub enum Row {
    Head(String),
    Item { label: String, hint: String, value: String, key: Option<char> },
}

pub fn item(label: impl Into<String>, hint: impl Into<String>, value: impl Into<String>, key: Option<char>) -> Row {
    Row::Item { label: label.into(), hint: hint.into(), value: value.into(), key }
}

struct Raw;
impl Raw {
    fn on() -> std::io::Result<Self> {
        terminal::enable_raw_mode()?;
        print!("\x1b[?25l");
        let _ = std::io::stdout().flush();
        Ok(Raw)
    }
}
impl Drop for Raw {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        print!("\x1b[?25h");
        let _ = std::io::stdout().flush();
    }
}

/// Piltangent-meny i släktens stil. Siffra väljer direkt, q/esc avbryter (→ None).
pub fn choose(rows: &[Row], msg: &str, mood: Mood, status: &[String], sub: &str) -> Option<String> {
    let sel: Vec<usize> =
        rows.iter().enumerate().filter(|(_, r)| matches!(r, Row::Item { .. })).map(|(i, _)| i).collect();
    if sel.is_empty() {
        return None;
    }
    let mut keys: Vec<(char, usize)> = Vec::new();
    let mut nums: Vec<Option<char>> = vec![None; rows.len()];
    let mut n = 0u32;
    for &k in &sel {
        if let Row::Item { key: Some(ch), .. } = &rows[k] {
            keys.push((*ch, k));
        } else {
            n += 1;
            if n <= 9 {
                let ch = char::from_digit(n, 10).unwrap();
                nums[k] = Some(ch);
                keys.push((ch, k));
            }
        }
    }
    let mut pos = sel[0];
    let mut top = 0usize;
    let mut live = Live::default();
    let t0 = Instant::now();
    let _raw = Raw::on().ok()?;

    let draw = |live: &mut Live, pos: usize, top: &mut usize, m: Mood| {
        let w = width();
        let mut lines = frame(msg, m, 0, "", sub);
        if !status.is_empty() {
            lines.push(String::new());
            lines.extend(status.iter().cloned());
        }
        lines.push(String::new());
        let narrow = w < 58;
        let maxl = sel
            .iter()
            .filter_map(|&k| if let Row::Item { label, .. } = &rows[k] { Some(label.chars().count()) } else { None })
            .max()
            .unwrap_or(10);
        let lw = (maxl + 2).min(if narrow { w.saturating_sub(14).max(10) } else { (w / 2).clamp(10, 34) });
        let room = height().saturating_sub(lines.len() + 3).max(3);
        *top = (*top).max(pos.saturating_sub(room - 1)).min(pos);
        *top = (*top).min(rows.len().saturating_sub(room));
        if *top > 0 {
            lines.push(dim("     ↑ fler"));
        }
        for (k, r) in rows.iter().enumerate().skip(*top).take(room) {
            match r {
                Row::Head(t) => lines.push(format!("   {}", dim(&cut(&t.to_uppercase(), w.saturating_sub(5))))),
                Row::Item { label, hint, key, .. } => {
                    let on = k == pos;
                    let kc = key.or(nums[k]).map(|c| c.to_string()).unwrap_or_else(|| " ".into());
                    let l = cut(label, lw - 1);
                    let l = if on { bold(&accent(&l)) } else { l };
                    let arrow = if on { accent("❯") } else { " ".into() };
                    let mut line = if narrow {
                        format!("  {arrow}{} {l}", dim(&kc))
                    } else {
                        format!("   {arrow} {}  {}", dim(&kc), pad(&l, lw))
                    };
                    if !narrow && !hint.is_empty() && w >= 64 {
                        line.push_str(&dim(hint));
                    }
                    lines.push(line);
                }
            }
        }
        if *top + room < rows.len() {
            lines.push(dim("     ↓ fler"));
        }
        lines.push(String::new());
        lines.push(dim(if w >= 50 {
            tr!("   ↑/↓ · enter · a digit picks · q quits", "   ↑/↓ · enter · siffra väljer · q avsluta")
        } else {
            "   ↑↓ enter · q"
        }));
        live.draw(&lines);
    };

    let mut size = (0, 0);
    let mut cur_mood = None;
    let choice = loop {
        let m = if anim() { idle_mood(t0.elapsed().as_secs_f64()) } else { mood };
        // glad: blinka och titta runt; annan min: behåll minen men blinka ändå
        let m = if mood == Mood::Happy || m == Mood::Blink { m } else { mood };
        if size != (width(), height()) || cur_mood != Some(m) {
            size = (width(), height());
            cur_mood = Some(m);
            draw(&mut live, pos, &mut top, m);
        }
        if !event::poll(Duration::from_millis(120)).unwrap_or(false) {
            continue;
        }
        let Ok(Event::Key(k)) = event::read() else { continue };
        if k.kind != KeyEventKind::Press {
            continue;
        }
        let idx = sel.iter().position(|&s| s == pos).unwrap_or(0);
        match k.code {
            KeyCode::Up | KeyCode::Char('k') => pos = sel[(idx + sel.len() - 1) % sel.len()],
            KeyCode::Down | KeyCode::Char('j') => pos = sel[(idx + 1) % sel.len()],
            KeyCode::Enter => break value_of(&rows[pos]),
            KeyCode::Esc => break None,
            KeyCode::Char('c' | 'd') if k.modifiers.contains(KeyModifiers::CONTROL) => break None,
            KeyCode::Char(ch) => {
                if let Some(&(_, k)) = keys.iter().find(|(c, _)| *c == ch) {
                    break value_of(&rows[k]);
                }
                if ch == 'q' {
                    break None;
                }
            }
            _ => {}
        }
        cur_mood = None;
    };
    live.clear();
    choice
}

fn value_of(r: &Row) -> Option<String> {
    match r {
        Row::Item { value, .. } => Some(value.clone()),
        Row::Head(_) => None,
    }
}

pub fn clear_screen() {
    if tty() {
        print!("\x1b[H\x1b[2J");
        let _ = std::io::stdout().flush();
    }
}

pub fn pause() {
    if std::io::stdin().is_terminal() {
        print!("{}", dim(tr!("\n   enter to continue ", "\n   enter för att fortsätta ")));
        let _ = std::io::stdout().flush();
        let mut s = String::new();
        let _ = std::io::stdin().read_line(&mut s);
    }
}
