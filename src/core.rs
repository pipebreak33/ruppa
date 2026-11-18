//! Släkten: vilka kusinerna är, var de bor, vilka versioner som finns och hur de installeras.
//! Beteendet är detsamma som i wuppa.py (github.com/pipebreak33/wuppa), som är facit.

use regex::Regex;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::thread;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const OWNER: &str = "pipebreak33";

/// En kusin: en egen Python-TUI med eget repo och egna installers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cousin {
    pub key: &'static str,
    pub name: &'static str,
    pub logo: &'static str,
    /// xterm-256-färger från ljus topp till mörkare botten (samma som i kusinens egen kod)
    pub shade: &'static [u8],
    pub art: &'static [&'static str],
    pub desc: &'static str,
}

pub const UPPA: Cousin = Cousin {
    key: "uppa",
    name: "Uppa",
    logo: "U›",
    shade: &[121, 85, 49, 43, 36],
    art: &["██     ██", "██{e}██", "██{m}██", "██▄   ▄██", " ▀█████▀ "],
    desc: "håller PowerShell- och Microsoft-verktygen uppdaterade",
};
pub const PUPPA: Cousin = Cousin {
    key: "puppa",
    name: "Puppa",
    logo: "P›",
    shade: &[117, 75, 69, 63],
    art: &["████████▄", "██{e}██", "██{m}██", "████████▀", "██       ", "▀▀       "],
    desc: "M365-admin i PowerShell med några knapptryck",
};
pub const MUPPA: Cousin = Cousin {
    key: "muppa",
    name: "Muppa",
    logo: "M›",
    shade: &[219, 213, 177, 141, 135, 99],
    art: &["██▀▄ ▄▀██", "██{e}██", "██{m}██", "██     ██", "██     ██", "▀▀     ▀▀"],
    desc: "flyttar en dator från lokal AD till Entra ID",
};
pub const EUPPA: Cousin = Cousin {
    key: "euppa",
    name: "Euppa",
    logo: "E›",
    shade: &[218, 212, 205, 199, 163, 126],
    art: &["█████████", "██{e}  ", "██{m}  ", "███████  ", "██       ", "█████████"],
    desc: "allt i Entra ID: användare, grupper, enheter, appar och roller",
};
/// Ruppa själv: ett R med ögon, i rost.
pub const RUPPA: Cousin = Cousin {
    key: "ruppa",
    name: "Ruppa",
    logo: "R›",
    shade: &[209, 208, 202, 166, 130, 94],
    art: &["████████▄", "██{e}██", "██{m}██", "████████▀", "██   ▀██▄", "▀▀     ▀▀"],
    desc: "Wuppa i Rust: ett fönster för hela släkten",
};

pub const COUSINS: [Cousin; 4] = [UPPA, PUPPA, MUPPA, EUPPA];

pub fn cousin(key: &str) -> Option<Cousin> {
    COUSINS.iter().copied().find(|c| c.key.eq_ignore_ascii_case(key))
}

/// Status för en kusin efter en skanning.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// installerad version (None = saknas)
    pub current: Option<String>,
    /// version på GitHub (None = gick inte att nå)
    pub latest: Option<String>,
}

impl Status {
    pub fn installed(&self) -> bool {
        self.current.is_some()
    }
    pub fn outdated(&self) -> bool {
        match (&self.current, &self.latest) {
            (Some(c), Some(l)) => newer(l, c),
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Scan {
    pub cousins: HashMap<&'static str, Status>,
    /// senaste Ruppa på GitHub Releases
    pub self_latest: Option<String>,
}

impl Scan {
    pub fn get(&self, key: &str) -> Status {
        self.cousins.get(key).cloned().unwrap_or_default()
    }
    pub fn missing(&self) -> Vec<Cousin> {
        COUSINS.iter().copied().filter(|c| !self.get(c.key).installed()).collect()
    }
    pub fn outdated(&self) -> Vec<Cousin> {
        COUSINS.iter().copied().filter(|c| self.get(c.key).outdated()).collect()
    }
    pub fn self_outdated(&self) -> bool {
        self.self_latest.as_deref().is_some_and(|l| newer(l, VERSION))
    }
    pub fn offline(&self) -> bool {
        COUSINS.iter().all(|c| self.get(c.key).latest.is_none())
    }
}

// ───────────────────────────── versioner ─────────────────────────────

fn num_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\d+").unwrap())
}

pub fn vtuple(v: &str) -> Vec<u64> {
    num_re().find_iter(v).take(4).filter_map(|m| m.as_str().parse().ok()).collect()
}

/// Är `latest` nyare än `current`? Numeriskt per del, så 1.10.0 > 1.9.3.
pub fn newer(latest: &str, current: &str) -> bool {
    let (a, b) = (vtuple(latest), vtuple(current));
    !a.is_empty() && a > b
}

/// `VERSION = "…"` i ett Python-skript.
pub fn parse_py_version(text: &str) -> Option<String> {
    static R: OnceLock<Regex> = OnceLock::new();
    let re = R.get_or_init(|| Regex::new(r#"(?m)^VERSION\s*=\s*["']([^"']+)"#).unwrap());
    re.captures(text).map(|c| c[1].to_string())
}

// ───────────────────────────── miljö ─────────────────────────────

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Kataloger där kusinerna (och Python) bor. En GUI-app ärver ofta inte skalets PATH, särskilt på macOS.
pub fn extra_path(home: &Path) -> Vec<PathBuf> {
    if cfg!(windows) {
        let mut v: Vec<PathBuf> = Vec::new();
        if let Some(la) = std::env::var_os("LOCALAPPDATA") {
            v.push(PathBuf::from(la).join("Microsoft").join("WindowsApps"));
        }
        v
    } else {
        let mut v = vec![home.join(".local/bin")];
        if cfg!(target_os = "macos") {
            v.push("/opt/homebrew/bin".into());
        }
        v.extend(["/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"].map(PathBuf::from));
        v
    }
}

/// Lägg till kusinernas kataloger i PATH för den här processen (och därmed alla barnprocesser).
pub fn augment_path() {
    let cur = std::env::var_os("PATH").unwrap_or_default();
    let mut parts: Vec<PathBuf> = vec![bin_dir()];
    parts.extend(std::env::split_paths(&cur).filter(|p| *p != bin_dir()));
    for p in extra_path(&home()) {
        if !parts.contains(&p) {
            parts.push(p);
        }
    }
    if let Ok(joined) = std::env::join_paths(parts) {
        // SAFETY: anropas en gång i början av main, innan några trådar har startats.
        unsafe { std::env::set_var("PATH", joined) };
    }
}

/// Hitta ett kommando i PATH. Windows: provar PATHEXT (.exe, .cmd …), så att .cmd-startare körs via full sökväg.
pub fn which(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .map(|s| s.to_ascii_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path) {
        for ext in &exts {
            let p = dir.join(format!("{cmd}{ext}"));
            if is_executable(&p) {
                return Some(p);
            }
        }
    }
    None
}

fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.metadata().map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// Bygg ett Command som fungerar även för .cmd/.bat på Windows.
pub fn command_for(exe: &Path) -> Command {
    let is_script =
        exe.extension().map(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat")).unwrap_or(false);
    if cfg!(windows) && is_script {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(exe);
        c
    } else {
        Command::new(exe)
    }
}

// ───────────────────────────── config ─────────────────────────────

/// ~/.wuppa/config.json – delas med Wuppa: {"repos": {"puppa": "ägare/repo"}, "local": {"puppa": "/sökväg"}}
pub fn config_path(home: &Path) -> PathBuf {
    home.join(".wuppa").join("config.json")
}

pub fn load_config(home: &Path) -> serde_json::Value {
    std::fs::read_to_string(config_path(home))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::Value::Null)
}

/// Repo och gren för ett verktyg: <NAMN>_REPO / <NAMN>_BRANCH, annars config.json, annars pipebreak33/<namn>@main.
pub fn repo_for(name: &str, env: &dyn Fn(&str) -> Option<String>, cfg: &serde_json::Value) -> (String, String) {
    let up = name.to_ascii_uppercase();
    let repo = env(&format!("{up}_REPO"))
        .filter(|s| !s.is_empty())
        .or_else(|| cfg["repos"][name].as_str().map(String::from))
        .unwrap_or_else(|| format!("{OWNER}/{name}"));
    let branch = env(&format!("{up}_BRANCH")).filter(|s| !s.is_empty()).unwrap_or_else(|| "main".into());
    (repo, branch)
}

fn real_env(k: &str) -> Option<String> {
    std::env::var(k).ok()
}

// ───────────────────────────── nät ─────────────────────────────

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(20))).build().into()
}

pub fn http_get(url: &str) -> Option<String> {
    let mut resp = agent()
        .get(url)
        .header("User-Agent", format!("ruppa/{VERSION}"))
        .header("Cache-Control", "no-cache")
        .call()
        .ok()?;
    resp.body_mut().with_config().limit(32 * 1024 * 1024).read_to_string().ok()
}

pub fn http_get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let mut resp =
        agent().get(url).header("User-Agent", format!("ruppa/{VERSION}")).call().map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    resp.body_mut().with_config().limit(200 * 1024 * 1024).reader().read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// Senaste version av en kusin: VERSION i <namn>.py på GitHub.
pub fn latest_version(name: &str) -> Option<String> {
    let (repo, branch) = repo_for(name, &real_env, &load_config(&home()));
    let text = http_get(&format!("https://raw.githubusercontent.com/{repo}/{branch}/{name}.py"))?;
    parse_py_version(&text)
}

/// Installerad version: VERSION i ~/.ruppa/<namn>/<namn>.py (ingen process behöver startas).
pub fn installed_version(name: &str) -> Option<String> {
    let text = std::fs::read_to_string(cousin_dir(name).join(format!("{name}.py"))).ok()?;
    if !shim_path(name).exists() {
        return None;
    }
    Some(parse_py_version(&text).unwrap_or_else(|| "?".into()))
}

/// Skanna alla kusiner och Ruppa själv parallellt.
pub fn scan() -> Scan {
    let mut handles = Vec::new();
    for c in COUSINS {
        handles.push((
            c.key,
            thread::spawn(move || installed_version(c.key)),
            thread::spawn(move || latest_version(c.key)),
        ));
    }
    let me = thread::spawn(crate::selfupdate::latest_release_version);
    let mut s = Scan::default();
    for (k, cur, lat) in handles {
        s.cousins.insert(k, Status { current: cur.join().ok().flatten(), latest: lat.join().ok().flatten() });
    }
    s.self_latest = me.join().ok().flatten();
    s
}

// ───────────────────────────── installation i ~/.ruppa ─────────────────────────────
//
// Allt bor i EN mapp:
//   ~/.ruppa/bin/      ruppa själv + startare för uppa, puppa och muppa (den här mappen ligger i PATH)
//   ~/.ruppa/<kusin>/  kusinens kod, uppackad från dess repo på GitHub
// RUPPA_HOME flyttar mappen. Kusinernas egna inställningar (t.ex. ~/.puppa/config.json) rör vi inte.

pub fn ruppa_dir() -> PathBuf {
    std::env::var_os("RUPPA_HOME").filter(|s| !s.is_empty()).map(PathBuf::from).unwrap_or_else(|| home().join(".ruppa"))
}

pub fn bin_dir() -> PathBuf {
    ruppa_dir().join("bin")
}

pub fn cousin_dir(name: &str) -> PathBuf {
    ruppa_dir().join(name)
}

pub fn shim_name(name: &str, windows: bool) -> String {
    if windows { format!("{name}.cmd") } else { name.to_string() }
}

pub fn shim_path(name: &str) -> PathBuf {
    bin_dir().join(shim_name(name, cfg!(windows)))
}

/// Startaren för en installerad kusin, eller None.
pub fn cousin_exe(name: &str) -> Option<PathBuf> {
    let p = shim_path(name);
    p.exists().then_some(p)
}

/// Innehållet i startaren: kör kusinens .py med den Python som hittades vid installationen.
pub fn shim_text(windows: bool, python: &[String], script: &Path) -> String {
    if windows {
        // Citera bara exe-filen: py-startaren skickar annars vidare ett citerat "-3" till python.exe
        let extra: String = python[1..].iter().map(|a| format!(" {a}")).collect();
        format!("@echo off\r\n\"{}\"{extra} \"{}\" %*\r\n", python[0], script.display())
    } else {
        let py: Vec<String> = python.iter().map(|p| sh_quote(p)).collect();
        format!("#!/bin/sh\nexec {} {} \"$@\"\n", py.join(" "), sh_quote(&script.to_string_lossy()))
    }
}

fn quiet(cmd: &mut Command) -> &mut Command {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: inget konsolfönster blinkar förbi från GUI:t
    }
    cmd
}

/// Python 3.8+ som [exe, extra argument …]. Microsoft Store-aliaset python.exe sorteras bort av testet.
pub fn find_python() -> Option<Vec<String>> {
    let mut cands: Vec<Vec<String>> = if cfg!(windows) {
        vec![vec!["py".into(), "-3".into()], vec!["python".into()], vec!["python3".into()]]
    } else {
        vec![vec!["python3".into()], vec!["python".into()]]
    };
    if cfg!(windows)
        && let Some(la) = std::env::var_os("LOCALAPPDATA")
    {
        let base = PathBuf::from(la).join("Programs").join("Python");
        cands.push(vec![base.join("Launcher").join("py.exe").to_string_lossy().into(), "-3".into()]);
        for v in ["Python313", "Python312", "Python311", "Python310"] {
            cands.push(vec![base.join(v).join("python.exe").to_string_lossy().into()]);
        }
    }
    for c in cands {
        let exe = if Path::new(&c[0]).is_absolute() { Some(PathBuf::from(&c[0])) } else { which(&c[0]) };
        let Some(exe) = exe.filter(|e| e.exists()) else { continue };
        let ok =
            quiet(Command::new(&exe).args(&c[1..]).args(["-c", "import sys; sys.exit(sys.version_info < (3, 8))"]))
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
        if ok {
            let mut v = vec![exe.to_string_lossy().to_string()];
            v.extend(c[1..].iter().cloned());
            return Some(v);
        }
    }
    None
}

/// Hitta Python, och på Windows: installera det per användare med winget om det saknas.
fn ensure_python(on_line: &mut dyn FnMut(String)) -> Result<Vec<String>, String> {
    if let Some(p) = find_python() {
        return Ok(p);
    }
    if cfg!(windows) {
        if let Some(wg) = which("winget") {
            on_line("Python saknas. Installerar Python 3.12 med winget (per användare, inga adminrättigheter)…".into());
            let _ = quiet(Command::new(wg).args([
                "install",
                "--id",
                "Python.Python.3.12",
                "-e",
                "--scope",
                "user",
                "--silent",
                "--accept-package-agreements",
                "--accept-source-agreements",
            ]))
            .status();
            if let Some(p) = find_python() {
                return Ok(p);
            }
        }
        return Err(
            "Python 3.8+ saknas. Installera det med: winget install Python.Python.3.12 (eller från python.org)".into(),
        );
    }
    if cfg!(target_os = "macos") {
        Err("Python 3.8+ saknas. Kör: xcode-select --install   (eller: brew install python)".into())
    } else {
        Err("Python 3.8+ saknas. Kör: sudo apt install python3   (eller dnf/pacman motsvarande)".into())
    }
}

fn expand_tilde(p: &str) -> PathBuf {
    match p.strip_prefix('~') {
        Some(rest) => home().join(rest.trim_start_matches(['/', '\\'])),
        None => PathBuf::from(p),
    }
}

/// Packa upp ett GitHub-arkiv (.tar.gz) i `dest` och hoppa över den översta mappen (<repo>-<gren>/).
pub fn unpack_strip(data: &[u8], dest: &Path) -> Result<usize, String> {
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(data));
    let mut n = 0;
    for e in ar.entries().map_err(|e| format!("trasigt arkiv: {e}"))? {
        let mut e = e.map_err(|e| format!("trasigt arkiv: {e}"))?;
        let path = e.path().map_err(|e| e.to_string())?.into_owned();
        let rel: PathBuf = path.components().skip(1).collect();
        if rel.as_os_str().is_empty() {
            continue;
        }
        // aldrig utanför målmappen
        if rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
            continue;
        }
        let out = dest.join(&rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        e.unpack(&out).map_err(|e| format!("kunde inte packa upp {}: {e}", rel.display()))?;
        n += 1;
    }
    Ok(n)
}

/// Kopiera en lokal utcheckning (config.json "local"), utan .git och cachefiler.
fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(src).map_err(|e| format!("{}: {e}", src.display()))? {
        let e = e.map_err(|e| e.to_string())?;
        let name = e.file_name();
        let n = name.to_string_lossy();
        if matches!(n.as_ref(), ".git" | "__pycache__" | ".DS_Store" | "target" | "node_modules") {
            continue;
        }
        let (from, to) = (e.path(), dst.join(&name));
        if e.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| format!("{}: {e}", from.display()))?;
        }
    }
    Ok(())
}

/// Installera (eller uppdatera) en kusin i ~/.ruppa. Rader om vad som händer skickas till `on_line`.
pub fn install_cousin(name: &str, on_line: &mut dyn FnMut(String)) -> bool {
    match install_inner(name, on_line) {
        Ok(v) => {
            on_line(format!("✔ {name} {v} installerad i {}", cousin_dir(name).display()));
            true
        }
        Err(e) => {
            on_line(format!("✘ {e}"));
            false
        }
    }
}

fn install_inner(name: &str, on_line: &mut dyn FnMut(String)) -> Result<String, String> {
    let py = ensure_python(on_line)?;
    let cfg = load_config(&home());
    let root = ruppa_dir();
    std::fs::create_dir_all(bin_dir()).map_err(|e| format!("kunde inte skapa {}: {e}", bin_dir().display()))?;
    let new = root.join(format!("{name}.new"));
    let _ = std::fs::remove_dir_all(&new);
    std::fs::create_dir_all(&new).map_err(|e| e.to_string())?;

    let res = (|| {
        if let Some(local) = cfg["local"][name].as_str() {
            let src = expand_tilde(local);
            on_line(format!("Kopierar {name} från {}", src.display()));
            copy_tree(&src, &new)?;
        } else {
            let (repo, branch) = repo_for(name, &real_env, &cfg);
            on_line(format!("Hämtar {name} från github.com/{repo} ({branch})"));
            let data = http_get_bytes(&format!("https://github.com/{repo}/archive/{branch}.tar.gz"))
                .map_err(|e| format!("kunde inte hämta github.com/{repo} ({branch}): {e}"))?;
            unpack_strip(&data, &new)?;
        }
        let script = new.join(format!("{name}.py"));
        let text = std::fs::read_to_string(&script).map_err(|_| format!("{name}.py saknas i det som hämtades"))?;
        // syntaxkontroll innan den fungerande versionen byts ut
        let ok = quiet(
            Command::new(&py[0])
                .args(&py[1..])
                .args(["-c", "import ast,sys; ast.parse(open(sys.argv[1],encoding='utf-8').read())"]),
        )
        .arg(&script)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
        if !ok {
            return Err(format!("{name}.py är trasig, behåller den gamla versionen"));
        }
        Ok(parse_py_version(&text).unwrap_or_else(|| "?".into()))
    })();
    let ver = match res {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&new);
            return Err(e);
        }
    };

    // byt ut atomiskt-ish: gammal → .old, ny → plats, radera .old
    let dst = cousin_dir(name);
    let old = root.join(format!("{name}.old"));
    let _ = std::fs::remove_dir_all(&old);
    if dst.exists() {
        std::fs::rename(&dst, &old).map_err(|e| format!("kunde inte flytta undan {}: {e}", dst.display()))?;
    }
    if let Err(e) = std::fs::rename(&new, &dst) {
        let _ = std::fs::rename(&old, &dst);
        return Err(format!("kunde inte flytta in {name}: {e}"));
    }
    let _ = std::fs::remove_dir_all(&old);

    let shim = shim_path(name);
    let _ = std::fs::remove_file(&shim); // en ev. symlänk: skriv aldrig genom den
    std::fs::write(&shim, shim_text(cfg!(windows), &py, &dst.join(format!("{name}.py"))))
        .map_err(|e| format!("kunde inte skriva {}: {e}", shim.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755));
    }
    Ok(ver)
}

/// Läs konstanter direkt ur en installerad kusin (t.ex. Muppas FOLDERS eller Puppas CAT_HINTS) som JSON.
/// Så är Ruppa bara ett skal: när kusinen uppdateras får sidorna de nya valen utan en ny Ruppa.
/// Modulen laddas utan att köras (main körs bara när filen startas som program).
pub fn describe(name: &str, keys: &[&str]) -> Option<serde_json::Value> {
    const PY: &str = "import importlib.util,json,sys,io,contextlib\n\
spec=importlib.util.spec_from_file_location('kusin',sys.argv[1])\n\
m=importlib.util.module_from_spec(spec)\n\
with contextlib.redirect_stdout(io.StringIO()): spec.loader.exec_module(m)\n\
print(json.dumps({k:getattr(m,k,None) for k in sys.argv[2].split(',')},ensure_ascii=False,default=list))";
    let script = cousin_dir(name).join(format!("{name}.py"));
    if !script.exists() {
        return None;
    }
    let py = find_python()?;
    let mut cmd = Command::new(&py[0]);
    cmd.args(&py[1..]).arg("-c").arg(PY).arg(&script).arg(keys.join(",")).env("PYTHONIOENCODING", "utf-8");
    cmd.stdin(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().ok().filter(|o| o.status.success())?;
    serde_json::from_slice(&out.stdout).ok()
}

pub fn strip_ansi(s: &str) -> String {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\x1b\[[0-9;?]*[a-zA-Z]|\x1b\][^\x07]*\x07").unwrap()).replace_all(s, "").into_owned()
}

// ───────────────────────────── starta i terminal ─────────────────────────────

/// Öppna en kusin (en TUI) i ett riktigt terminalfönster.
pub fn launch_in_terminal(name: &str) -> Result<(), String> {
    let exe = cousin_exe(name).ok_or_else(|| format!("{name} är inte installerad"))?;
    let exe_s = exe.to_string_lossy().to_string();
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell application \"Terminal\"\n activate\n do script \"exec {}\"\nend tell",
            sh_quote(&exe_s).replace('\\', "\\\\").replace('"', "\\\"")
        );
        let ok = Command::new("osascript").arg("-e").arg(script).status().map(|s| s.success()).unwrap_or(false);
        if ok { Ok(()) } else { Err("kunde inte öppna Terminal".into()) }
    }
    #[cfg(windows)]
    {
        if let Some(wt) = which("wt") {
            if Command::new(wt).args(["new-tab", "--title", name, "cmd", "/k"]).arg(&exe_s).spawn().is_ok() {
                return Ok(());
            }
        }
        return Command::new("cmd")
            .args(["/c", "start", name, "cmd", "/k"])
            .arg(&exe_s)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let tries: [(&str, &[&str]); 5] = [
            ("x-terminal-emulator", &["-e"]),
            ("gnome-terminal", &["--"]),
            ("konsole", &["-e"]),
            ("xfce4-terminal", &["-x"]),
            ("xterm", &["-e"]),
        ];
        for (term, args) in tries {
            if let Some(t) = which(term) {
                if Command::new(t).args(args).arg(&exe_s).spawn().is_ok() {
                    return Ok(());
                }
            }
        }
        Err("hittade ingen terminal (x-terminal-emulator, gnome-terminal, konsole, xterm)".into())
    }
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

pub fn join_names(names: &[&str]) -> String {
    match names.len() {
        0 => String::new(),
        1 => names[0].to_string(),
        n => format!("{} och {}", names[..n - 1].join(", "), names[n - 1]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn version_compare() {
        assert!(newer("1.10.0", "1.9.3"));
        assert!(newer("2.0", "1.99.99"));
        assert!(newer("1.0.1", "1.0"));
        assert!(!newer("1.0.0", "1.0.0"));
        assert!(!newer("1.9.3", "1.10.0"));
        assert!(!newer("", "1.0.0"));
    }

    #[test]
    fn version_parse() {
        assert_eq!(parse_py_version("x = 1\nVERSION = \"1.2.3\"\n").as_deref(), Some("1.2.3"));
        assert_eq!(parse_py_version("VERSION='0.9'").as_deref(), Some("0.9"));
        assert_eq!(parse_py_version("MY_VERSION = \"1\""), None);
    }

    #[test]
    fn repo_resolution() {
        let none = |_: &str| None;
        assert_eq!(repo_for("puppa", &none, &json!(null)), ("pipebreak33/puppa".into(), "main".into()));
        let cfg = json!({"repos": {"puppa": "annan/puppa"}});
        assert_eq!(repo_for("puppa", &none, &cfg).0, "annan/puppa");
        let env = |k: &str| match k {
            "PUPPA_REPO" => Some("env/puppa".to_string()),
            "PUPPA_BRANCH" => Some("dev".to_string()),
            _ => None,
        };
        assert_eq!(repo_for("puppa", &env, &cfg), ("env/puppa".into(), "dev".into()));
        let empty = |_: &str| Some(String::new());
        assert_eq!(repo_for("uppa", &empty, &json!(null)), ("pipebreak33/uppa".into(), "main".into()));
    }

    #[test]
    fn shims() {
        let unix = shim_text(false, &["/usr/bin/python3".into()], Path::new("/h/.ruppa/uppa/uppa.py"));
        assert_eq!(unix, "#!/bin/sh\nexec '/usr/bin/python3' '/h/.ruppa/uppa/uppa.py' \"$@\"\n");
        let win = shim_text(true, &[r"C:\py.exe".into(), "-3".into()], Path::new(r"C:\u\.ruppa\uppa\uppa.py"));
        assert_eq!(win, "@echo off\r\n\"C:\\py.exe\" -3 \"C:\\u\\.ruppa\\uppa\\uppa.py\" %*\r\n");
        assert_eq!(shim_name("muppa", true), "muppa.cmd");
        assert_eq!(shim_name("muppa", false), "muppa");
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn unpack_strips_top_dir_and_refuses_escapes() {
        let mut tarbuf = Vec::new();
        {
            let gz = flate2::write::GzEncoder::new(&mut tarbuf, flate2::Compression::fast());
            let mut b = tar::Builder::new(gz);
            for (path, body) in [("puppa-main/puppa.py", "VERSION = \"9.9\"\n"), ("puppa-main/recipes/a.json", "{}")] {
                let mut h = tar::Header::new_gnu();
                h.set_size(body.len() as u64);
                h.set_mode(0o644);
                h.set_cksum();
                b.append_data(&mut h, path, body.as_bytes()).unwrap();
            }
            b.into_inner().unwrap().finish().unwrap();
        }
        let dir = std::env::temp_dir().join(format!("ruppa-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(unpack_strip(&tarbuf, &dir).unwrap(), 2);
        assert!(dir.join("puppa.py").exists());
        assert!(dir.join("recipes/a.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_helpers() {
        let mut s = Scan::default();
        s.cousins.insert("uppa", Status { current: Some("1.0.0".into()), latest: Some("1.1.0".into()) });
        s.cousins.insert("puppa", Status { current: Some("1.3.0".into()), latest: Some("1.3.0".into()) });
        assert_eq!(s.outdated(), vec![UPPA]);
        assert_eq!(s.missing(), vec![MUPPA, EUPPA]);
        assert!(!s.offline());
    }

    #[test]
    fn names_and_ansi() {
        assert_eq!(join_names(&["Uppa"]), "Uppa");
        assert_eq!(join_names(&["Uppa", "Puppa", "Muppa"]), "Uppa, Puppa och Muppa");
        assert_eq!(strip_ansi("\x1b[38;5;42m✔\x1b[0m klart"), "✔ klart");
    }

    #[test]
    fn art_is_rectangular() {
        for c in COUSINS.iter().chain([&RUPPA]) {
            for row in c.art {
                let w = row.replace("{e}", "     ").replace("{m}", "     ").chars().count();
                assert_eq!(w, 9, "{} rad {row:?}", c.name);
            }
        }
    }
}
