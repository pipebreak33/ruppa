//! Ruppa – Wuppa i Rust. Ett fönster (och en CLI) för hela släkten: Uppa, Puppa, Muppa och Euppa.
//!
//!   ruppa                     fönstret (TUI-meny i terminalen när inget fönster går att öppna)
//!   ruppa status              vilka kusiner som finns och om de kan uppdateras
//!   ruppa install [namn …]    installera kusiner som saknas
//!   ruppa update [namn …]     uppdatera det som har en nyare version, och Ruppa själv
//!   ruppa uppa|puppa|muppa|euppa …  kör en kusin, allt efter namnet skickas vidare
//!   ruppa self-update         uppdatera bara Ruppa
//!   ruppa uninstall [-y]      ta bort Ruppa och kusinerna (dina data behålls)
//!   ruppa uninstall puppa …   ta bara bort de här kusinerna ur Ruppa

// Windows: en fönsterapp (inget konsolfönster blinkar till från Startmenyn). CLI-kommandona kopplar sig till
// terminalen de startas från, se attach_console().
#![cfg_attr(windows, windows_subsystem = "windows")]

/// The text in the chosen language (English first). Works for &str and String alike.
macro_rules! tr {
    ($en:expr, $sv:expr $(,)?) => {
        if crate::core::is_sv() { $sv } else { $en }
    };
}

mod core;
mod euppa_page;
mod fuzzy;
mod gui;
mod pages;
mod selfupdate;
mod term;
mod termview;
mod theme;
mod users;

use crate::core::{COUSINS, Cousin, VERSION};
use crate::term::{Mood, bold, c256, dim, red, say};
use clap::{Parser, Subcommand};
use std::io::IsTerminal;
use std::process::ExitCode;

const JOKES_EN: [&str; 6] = [
    "I'm Wuppa in Rust. Same troupe, new armour.",
    "An R with eyes. R as in Rust, or Rad. You choose.",
    "I need no Python. The others bring their own.",
    "Uppa updates, Puppa administers, Muppa migrates, Euppa runs Entra ID. I keep the troupe in order.",
    "Compiled, tested and clippy-approved. You can't ask more of an R.",
    "Run the same line again and everything updates. That's the whole trick.",
];
const JOKES_SV: [&str; 6] = [
    "Jag är Wuppa i Rust. Samma släkt, ny rustning.",
    "Ett R med ögon. R som i Rust, eller Rolig. Du väljer.",
    "Ingen Python behövs för mig. Kusinerna tar med sin egen.",
    "Uppa uppdaterar, Puppa administrerar, Muppa migrerar. Jag håller ordning på släkten.",
    "Kompilerad, testad och clippy-godkänd. Mer kan man inte begära av ett R.",
    "Kör samma rad igen så uppdateras allt. Det är hela tricket.",
];

#[derive(Parser)]
#[command(name = "ruppa", version, about = "Ruppa – Wuppa in Rust: Uppa, Puppa, Muppa and Euppa in one window")]
struct Cli {
    /// no animations (also off with NO_COLOR and when output is piped)
    #[arg(long, global = true)]
    no_anim: bool,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// which tools are installed and whether they can be updated
    Status,
    /// install missing tools (without names: all that are missing)
    Install { names: Vec<String> },
    /// update what has a newer version, and Ruppa itself
    #[command(alias = "upgrade")]
    Update {
        names: Vec<String>,
        /// reinstall even when the version is already the latest (with names)
        #[arg(short, long)]
        force: bool,
    },
    /// update only Ruppa
    SelfUpdate {
        /// only show what would happen
        #[arg(short = 'n', long)]
        dry_run: bool,
    },
    /// remove Ruppa and the tools, or only the tools you name (your data in ~/.puppa, ~/.muppa, ~/.euppa and
    /// ~/Documents is kept)
    Uninstall {
        /// only these tools (uppa, puppa, muppa, euppa); without names: all of Ruppa
        names: Vec<String>,
        /// don't ask
        #[arg(short, long)]
        yes: bool,
    },
    /// open the window
    Gui,
    /// the menu in the terminal
    Tui,
    /// show or set the language (en, sv) for the whole troupe
    #[command(alias = "språk", alias = "sprak")]
    Lang {
        #[arg(value_parser = ["en", "sv"])]
        value: Option<String>,
    },
}

#[cfg(windows)]
fn attach_console() {
    unsafe extern "system" {
        fn AttachConsole(pid: u32) -> i32;
    }
    // ATTACH_PARENT_PROCESS: skriv i terminalen som startade oss (om någon gjorde det)
    unsafe {
        AttachConsole(u32::MAX);
    }
}

fn main() -> ExitCode {
    core::augment_path();
    core::init_lang();
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(windows)]
    if !args.is_empty() && args[0] != "gui" {
        attach_console();
    }
    // ruppa puppa … → skicka allt vidare till kusinen
    if let Some(c) = args.first().and_then(|a| core::cousin(a)) {
        term::init(true);
        return code(run_cousin(&c, &args[1..]));
    }
    let cli = Cli::parse();
    term::init(cli.no_anim);
    let rc = match cli.cmd {
        None if gui_available() => gui::run(),
        None => menu(),
        Some(Cmd::Gui) => gui::run(),
        Some(Cmd::Tui) => menu(),
        Some(Cmd::Status) => cmd_status(),
        Some(Cmd::Install { names }) => cmd_install(&names),
        Some(Cmd::Update { names, force }) => cmd_update(&names, force),
        Some(Cmd::SelfUpdate { dry_run }) => cmd_self_update(dry_run),
        Some(Cmd::Uninstall { names, yes }) => cmd_uninstall(&names, yes),
        Some(Cmd::Lang { value }) => cmd_lang(value.as_deref()),
    };
    code(rc)
}

fn code(rc: i32) -> ExitCode {
    ExitCode::from(rc.clamp(0, 255) as u8)
}

/// Finns en grafisk miljö? (ssh utan X/Wayland → nej, då blir det TUI-menyn)
fn gui_available() -> bool {
    let has = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty());
    if cfg!(windows) {
        return true;
    }
    if cfg!(target_os = "macos") {
        return !(has("SSH_CONNECTION") || has("SSH_TTY"));
    }
    has("DISPLAY") || has("WAYLAND_DISPLAY")
}

fn pick(names: &[String]) -> Result<Vec<Cousin>, i32> {
    let mut out: Vec<Cousin> = Vec::new();
    for n in names {
        match core::cousin(n) {
            Some(c) if !out.contains(&c) => out.push(c),
            Some(_) => {}
            None => {
                eprintln!(
                    "{}",
                    tr!(
                        format!("  unknown tool: {n} (choose from uppa, puppa, muppa, euppa)"),
                        format!("  okänd kusin: {n} (välj bland uppa, puppa, muppa, euppa)")
                    )
                );
                return Err(2);
            }
        }
    }
    Ok(out)
}

fn scan_with_spinner(msg: &str) -> core::Scan {
    term::with_spinner(msg, core::scan)
}

/// Kör kusinernas egna installers en i taget, med utdata direkt i terminalen. Returnerar de som misslyckades.
fn do_install(targets: &[Cousin], verb: &str) -> Vec<Cousin> {
    let mut failed = Vec::new();
    for k in targets {
        println!();
        println!(
            "  {} {}",
            c256(k.shade[2.min(k.shade.len() - 1)], &bold(k.logo)),
            bold(&format!("{verb} {}", k.name))
        );
        let ok = core::install_cousin(k.key, &mut |line| println!("  {line}"));
        if !ok {
            println!("  {} {} strulade.", red("✘"), k.name);
            failed.push(*k);
        }
    }
    failed
}

fn cmd_lang(value: Option<&str>) -> i32 {
    if let Some(v) = value
        && let Err(e) = core::set_lang(v == "sv")
    {
        eprintln!("  {e}");
        return 1;
    }
    println!(
        "{}",
        tr!("  Language: English · change with: ruppa lang sv", "  Språk: svenska · byt med: ruppa lang en")
    );
    0
}

fn names_of(v: &[Cousin]) -> String {
    core::join_names(&v.iter().map(|c| c.name).collect::<Vec<_>>())
}

fn report(targets: &[Cousin], failed: &[Cousin], what: &str) -> i32 {
    if !failed.is_empty() {
        say(
            &tr!(
                format!("{} had trouble. The rest went fine. Run the same command again in a while.", names_of(failed)),
                format!("{} strulade. Resten gick bra. Kör samma kommando igen om en stund.", names_of(failed))
            ),
            Mood::Sad,
        );
        return 1;
    }
    if !targets.is_empty() {
        // "installerade" → "installerad" när det bara är en kusin
        let what = if targets.len() == 1 { what.strip_suffix('e').unwrap_or(what) } else { what };
        term::celebrate(&tr!(
            format!("Done! {} {} {what}.", names_of(targets), if targets.len() == 1 { "is" } else { "are" }),
            format!("Klart! {} är {what}.", names_of(targets))
        ));
    }
    0
}

fn cmd_status() -> i32 {
    let s = scan_with_spinner(tr!("Calling round the tools…", "Ringer runt till kusinerna…"));
    println!();
    for l in term::status_lines(&s, true) {
        println!("{}", term::fit(&l, term::width()));
    }
    println!(
        "{}{}",
        dim(&format!("\n  ruppa {VERSION}")),
        if s.offline() { dim(tr!(" · could not reach GitHub", " · kunde inte nå GitHub")) } else { String::new() }
    );
    0
}

fn cmd_install(names: &[String]) -> i32 {
    let chosen = match pick(names) {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    let s =
        scan_with_spinner(tr!("Checking which tools already live here…", "Kollar vilka kusiner som redan bor här…"));
    let targets: Vec<Cousin> = if chosen.is_empty() { s.missing() } else { chosen };
    if targets.is_empty() {
        say(
            tr!(
                "All the tools are already installed. To update them: ruppa update",
                "Alla kusinerna är redan installerade. Vill du uppdatera dem: ruppa update"
            ),
            Mood::Proud,
        );
        return 0;
    }
    let failed = do_install(&targets, tr!("Installing", "Installerar"));
    report(&targets, &failed, tr!("installed", "installerade"))
}

fn cmd_update(names: &[String], force: bool) -> i32 {
    let chosen = match pick(names) {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    let s = scan_with_spinner(tr!("Asking GitHub if there is anything new…", "Frågar GitHub om det finns nytt…"));
    let pool: Vec<Cousin> = if chosen.is_empty() { COUSINS.to_vec() } else { chosen.clone() };
    let targets: Vec<Cousin> = pool
        .iter()
        .copied()
        .filter(|k| {
            let st = s.get(k.key);
            st.outdated() || (force && !chosen.is_empty() && st.installed())
        })
        .collect();
    let missing: Vec<Cousin> = pool.iter().copied().filter(|k| !s.get(k.key).installed()).collect();
    let self_new = s.self_outdated() && chosen.is_empty();
    if targets.is_empty() && !self_new {
        let mut msg = tr!("Everything is already the latest version.", "Allt är redan senaste versionen.").to_string();
        if !missing.is_empty() {
            msg += &tr!(
                format!(" {} missing: ruppa install", names_of(&missing)),
                format!(" {} saknas: ruppa install", names_of(&missing))
            );
        }
        say(&msg, Mood::Proud);
        return 0;
    }
    let failed = do_install(&targets, tr!("Updating", "Uppdaterar"));
    let mut rc = report(&targets, &failed, tr!("updated", "uppdaterade"));
    if self_new {
        rc = rc.max(cmd_self_update(false));
    }
    rc
}

fn cmd_self_update(dry_run: bool) -> i32 {
    println!();
    println!("  {} {}", term::accent(&bold(core::RUPPA.logo)), bold("Uppdaterar Ruppa"));
    match selfupdate::self_update(dry_run, &mut |l| println!("  {l}")) {
        Ok(Some(v)) => {
            term::celebrate(&tr!(format!("Now I'm Ruppa {v}. Good as new!"), format!("Nu är jag Ruppa {v}. Som ny!")));
            0
        }
        Ok(None) => 0,
        Err(e) => {
            println!("  {} {e}", red("✘"));
            1
        }
    }
}

/// Avinstallera: utan namn hela Ruppa (avinstallationsskriptet), med namn bara de kusinerna ur ~/.ruppa.
fn cmd_uninstall(names: &[String], yes: bool) -> i32 {
    if names.is_empty() {
        let mut cmd = match core::uninstall_script(yes) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("  {} {e}", red("✘"));
                return 1;
            }
        };
        return cmd.status().map(|s| s.code().unwrap_or(1)).unwrap_or(1);
    }
    let mut chosen: Vec<Cousin> = Vec::new();
    for n in names {
        match core::cousin(n) {
            Some(c) if !chosen.contains(&c) => chosen.push(c),
            Some(_) => {}
            None => {
                eprintln!(
                    "{}",
                    tr!(
                        format!("  unknown tool: {n} (choose from uppa, puppa, muppa, euppa)"),
                        format!("  okänd kusin: {n} (välj bland uppa, puppa, muppa, euppa)")
                    )
                );
                return 2;
            }
        }
    }
    let installed: Vec<Cousin> = chosen.iter().copied().filter(|c| core::cousin_exe(c.key).is_some()).collect();
    if installed.is_empty() {
        say(
            tr!(
                "None of them is installed in Ruppa. Nothing to remove.",
                "Ingen av dem är installerad i Ruppa. Inget att ta bort."
            ),
            Mood::Happy,
        );
        return 0;
    }
    let list = names_of(&installed);
    if !yes {
        if !std::io::stdin().is_terminal() {
            eprintln!(
                "{}",
                tr!(
                    "  No terminal to ask in. Run with -y to uninstall without asking.".to_string(),
                    "  Ingen terminal att fråga i. Kör med -y för att avinstallera utan fråga.".to_string()
                )
            );
            return 1;
        }
        say(
            &tr!(
                format!("Uninstall {list} from Ruppa? Their data (tenants, templates, reports) is kept."),
                format!("Avinstallera {list} ur Ruppa? Deras data (tenants, mallar, rapporter) behålls.")
            ),
            Mood::Worry,
        );
        print!("  Avinstallera? [j/N] ");
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let mut s = String::new();
        let _ = std::io::stdin().read_line(&mut s);
        if !matches!(s.trim().to_lowercase().as_str(), "j" | "ja" | "y" | "yes") {
            say(tr!("Okay, nothing removed.", "Okej, inget borttaget."), Mood::Happy);
            return 0;
        }
    }
    let mut rc = 0;
    for c in &installed {
        match core::uninstall_cousin(c.key) {
            Ok(()) => println!("  {} {} avinstallerad", term::green("✔"), c.name),
            Err(e) => {
                eprintln!("  {} {e}", red("✘"));
                rc = 1;
            }
        }
    }
    if rc == 0 {
        say(
            &tr!(
                format!("Done! Removed {list} from Ruppa. Install again: ruppa install"),
                format!("Klart! {list} är borta ur Ruppa. Installera igen: ruppa install")
            ),
            Mood::Proud,
        );
    }
    rc
}

fn run_cousin(c: &Cousin, args: &[String]) -> i32 {
    let Some(exe) = core::cousin_exe(c.key) else {
        say(
            &tr!(
                format!("{} is not installed yet. Run: ruppa install {}", c.name, c.key),
                format!("{} är inte installerad än. Kör: ruppa install {}", c.name, c.key)
            ),
            Mood::Worry,
        );
        return 1;
    };
    match core::command_for(&exe).args(args).status() {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!(
                "{}",
                tr!(
                    format!("  {} could not start {}: {e}", red("✘"), c.name),
                    format!("  {} kunde inte starta {}: {e}", red("✘"), c.name)
                )
            );
            1
        }
    }
}

/// `joke`: vilket skämt (väljs en gång per skanning, så att pratbubblan står still), None = inget skämt.
pub fn greeting(s: &core::Scan, joke: Option<usize>) -> (String, Mood) {
    let missing = s.missing();
    let mut old: Vec<&str> = s.outdated().iter().map(|c| c.name).collect();
    if s.self_outdated() {
        old.push(tr!("myself", "mig själv"));
    }
    if missing.len() == COUSINS.len() {
        return (
            tr!(
                "Hi! I'm Ruppa. I keep the tools Uppa, Puppa, Muppa and Euppa together, but none of them live here yet. \
                 Shall we fetch them?",
                "Hej! Jag är Ruppa. Jag håller ihop verktygen Uppa, Puppa, Muppa och Euppa, men inget av dem bor här än. \
                 Ska vi hämta hem dem?"
            )
            .into(),
            Mood::Wow,
        );
    }
    if !missing.is_empty() {
        let n = names_of(&missing);
        let pron = if missing.len() == 1 { tr!("it", "den") } else { tr!("them", "dem") };
        return (
            tr!(
                format!("Hi! I'm Ruppa. {n} is still missing from the troupe. Shall I install {pron}?"),
                format!("Hej! Jag är Ruppa. {n} saknas fortfarande i släkten. Ska jag installera {pron}?")
            ),
            Mood::Worry,
        );
    }
    if !old.is_empty() {
        return (
            tr!(
                format!("Hi! I'm Ruppa. There is something new for {}. Shall we ruppa?", core::join_names(&old)),
                format!("Hej! Jag är Ruppa. Det finns nytt för {}. Ska vi ruppa?", core::join_names(&old))
            ),
            Mood::Wow,
        );
    }
    let joke = joke.map(|i| format!(" {}", tr!(JOKES_EN, JOKES_SV)[i % JOKES_EN.len()])).unwrap_or_default();
    (
        tr!(
            format!("Hi! I'm Ruppa. All the tools are home and fresh.{joke}"),
            format!("Hej! Jag är Ruppa. Alla kusinerna är hemma och fräscha.{joke}")
        ),
        Mood::Happy,
    )
}

/// Ett slumpat skämtnummer (en gång per skanning).
pub fn joke_seed() -> usize {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos() as usize).unwrap_or(0)
}

/// TUI-menyn, som Wuppas.
fn menu() -> i32 {
    if !std::io::stdin().is_terminal() || !term::tty() {
        return cmd_status();
    }
    term::clear_screen();
    let mut s = scan_with_spinner(tr!("Calling round the tools…", "Ringer runt till kusinerna…"));
    loop {
        let (msg, mood) = greeting(&s, (term::width() >= 80).then(joke_seed));
        let missing = s.missing();
        let old = s.outdated();
        let mut rows = vec![term::Row::Head(tr!("Start", "Starta").into())];
        for k in COUSINS {
            if s.get(k.key).installed() {
                rows.push(term::item(k.name, k.desc(), format!("run:{}", k.key), None));
            }
        }
        rows.push(term::Row::Head(tr!("The troupe", "Släkten").into()));
        let n_new = old.len() + usize::from(s.self_outdated());
        if n_new > 0 {
            rows.push(term::item(
                tr!("Update everything", "Uppdatera allt"),
                tr!(format!("{n_new} can be updated"), format!("{n_new} kan uppdateras")),
                "update",
                Some('u'),
            ));
        }
        if !missing.is_empty() {
            let label = if missing.len() == COUSINS.len() {
                tr!("Install all", "Installera alla").into()
            } else {
                format!("Installera {}", names_of(&missing))
            };
            rows.push(term::item(
                label,
                tr!("runs the tools' own installers", "kör kusinernas egna installers"),
                "install",
                Some('i'),
            ));
        }
        for k in COUSINS {
            let st = s.get(k.key);
            if st.installed() {
                let hint = if st.outdated() {
                    format!("{} → {}", st.current.unwrap_or_default(), st.latest.unwrap_or_default())
                } else {
                    tr!("reinstall the latest", "installera om senaste").into()
                };
                rows.push(term::item(
                    tr!(format!("Update {}", k.name), format!("Uppdatera {}", k.name)),
                    hint,
                    format!("one:{}", k.key),
                    None,
                ));
            }
        }
        let self_hint = match &s.self_latest {
            Some(l) if s.self_outdated() => format!("{VERSION} → {l}"),
            _ => tr!("already the latest", "redan senaste").into(),
        };
        rows.push(term::item(tr!("Update Ruppa", "Uppdatera Ruppa"), self_hint, "self", None));
        if gui_available() {
            rows.push(term::item(tr!("Open the window", "Öppna fönstret"), "", "gui", Some('g')));
        }
        rows.push(term::item(tr!("Scan again", "Skanna om"), "", "rescan", Some('r')));
        rows.push(term::item(
            tr!("Language: English", "Språk: svenska"),
            tr!("switch to Swedish (all the tools)", "byt till engelska (alla verktygen)"),
            "lang",
            Some('s'),
        ));
        rows.push(term::item(tr!("Quit", "Avsluta"), "", "quit", Some('q')));

        term::clear_screen();
        let sub = tr!(format!("ruppa {VERSION} · Wuppa in Rust"), format!("ruppa {VERSION} · Wuppa i Rust"));
        let v = term::choose(&rows, &msg, mood, &term::status_lines(&s, false), &sub);
        let Some(v) = v.filter(|v| v != "quit") else { return 0 };
        term::clear_screen();
        match v.as_str() {
            "lang" => {
                let _ = core::set_lang(!core::is_sv());
                continue;
            }
            "update" => {
                let failed = do_install(&old, tr!("Updating", "Uppdaterar"));
                report(&old, &failed, tr!("updated", "uppdaterade"));
                if s.self_outdated() {
                    cmd_self_update(false);
                }
                term::pause();
            }
            "install" => {
                let failed = do_install(&missing, tr!("Installing", "Installerar"));
                report(&missing, &failed, tr!("installed", "installerade"));
                term::pause();
            }
            "self" => {
                cmd_self_update(false);
                term::pause();
            }
            "gui" => return gui::run(),
            "rescan" => {}
            _ => {
                if let Some(key) = v.strip_prefix("run:") {
                    run_cousin(&core::cousin(key).unwrap(), &[]);
                } else if let Some(key) = v.strip_prefix("one:") {
                    let k = core::cousin(key).unwrap();
                    let failed = do_install(&[k], tr!("Updating", "Uppdaterar"));
                    report(&[k], &failed, tr!("updated", "uppdaterade"));
                    term::pause();
                }
            }
        }
        s = scan_with_spinner(tr!("Counting the tools again…", "Räknar in kusinerna igen…"));
    }
}
