//! Ruppa uppdaterar sig själv från GitHub Releases: rätt binär för plattformen, SHA-256 kontrolleras
//! mot SHA256SUMS i releasen, och binären byts ut atomiskt (self-replace sköter Windows, där den körande
//! exe-filen inte kan skrivas över).

use crate::core::{self, VERSION};
use sha2::{Digest, Sha256};

pub fn repo() -> String {
    std::env::var("RUPPA_REPO").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| format!("{}/ruppa", core::OWNER))
}

/// Namnet på release-filen för den här plattformen.
pub fn asset_name(os: &str, arch: &str) -> Option<String> {
    let arch = match arch {
        "x86_64" | "amd64" => "x86_64",
        "aarch64" | "arm64" => "aarch64",
        _ => return None,
    };
    match os {
        "macos" => Some("ruppa-macos-universal".into()),
        "linux" => Some(format!("ruppa-linux-{arch}")),
        "windows" => Some(format!("ruppa-windows-{arch}.exe")),
        _ => None,
    }
}

fn this_asset() -> Option<String> {
    asset_name(std::env::consts::OS, std::env::consts::ARCH)
}

fn latest_release() -> Option<serde_json::Value> {
    let text = core::http_get(&format!("https://api.github.com/repos/{}/releases/latest", repo()))?;
    serde_json::from_str(&text).ok()
}

pub fn tag_version(tag: &str) -> String {
    tag.trim_start_matches(|c: char| !c.is_ascii_digit()).to_string()
}

/// Senaste släppta version, eller None (inget nät, inga releaser än).
pub fn latest_release_version() -> Option<String> {
    latest_release()?["tag_name"].as_str().map(tag_version).filter(|v| !v.is_empty())
}

/// Hitta kontrollsumman för `name` i en SHA256SUMS-fil ("<hex>  <namn>" per rad).
pub fn sum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (h, n) = (it.next()?, it.next()?);
        (n.trim_start_matches('*') == name).then(|| h.to_ascii_lowercase())
    })
}

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}

/// Uppdatera Ruppa. `say` får en rad i taget. Ok(Some(ny version)) när binären har bytts ut.
pub fn self_update(dry_run: bool, say: &mut dyn FnMut(String)) -> Result<Option<String>, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if exe.components().any(|c| c.as_os_str() == "target") && exe.ancestors().any(|a| a.join("Cargo.toml").exists()) {
        return Err("det här är en utvecklingskopia (cargo build). Uppdatera med git pull + cargo build.".into());
    }
    let rel = latest_release().ok_or("kunde inte nå GitHub Releases (eller så finns ingen release än)")?;
    let tag = rel["tag_name"].as_str().unwrap_or_default();
    let latest = tag_version(tag);
    if !core::newer(&latest, VERSION) {
        say(format!("Ruppa {VERSION} är redan senaste versionen."));
        return Ok(None);
    }
    let name = this_asset().ok_or("ingen färdig binär för den här plattformen")?;
    let assets = rel["assets"].as_array().cloned().unwrap_or_default();
    let url_of = |n: &str| {
        assets
            .iter()
            .find(|a| a["name"].as_str() == Some(n))
            .and_then(|a| a["browser_download_url"].as_str().map(String::from))
    };
    let bin_url = url_of(&name).ok_or_else(|| format!("releasen {tag} saknar {name}"))?;
    let sums_url = url_of("SHA256SUMS").ok_or_else(|| format!("releasen {tag} saknar SHA256SUMS"))?;
    if dry_run {
        say(format!("skulle hämta {bin_url} och byta ut {}", exe.display()));
        return Ok(None);
    }
    say(format!("Hämtar Ruppa {latest} ({name})"));
    let sums = String::from_utf8(core::http_get_bytes(&sums_url)?).map_err(|e| e.to_string())?;
    let want = sum_for(&sums, &name).ok_or_else(|| format!("SHA256SUMS saknar {name}"))?;
    let data = core::http_get_bytes(&bin_url)?;
    let got = sha256_hex(&data);
    if got != want {
        return Err(format!("kontrollsumman stämmer inte ({got} ≠ {want}). Inget har ändrats."));
    }
    let tmp = exe.with_extension("new");
    std::fs::write(&tmp, &data).map_err(|e| format!("kunde inte skriva {}: {e}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
    }
    let res = self_replace::self_replace(&tmp).map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&tmp);
    res?;
    say(format!("Ruppa uppdaterad: {VERSION} → {latest}"));
    Ok(Some(latest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_per_platform() {
        assert_eq!(asset_name("macos", "aarch64").as_deref(), Some("ruppa-macos-universal"));
        assert_eq!(asset_name("macos", "x86_64").as_deref(), Some("ruppa-macos-universal"));
        assert_eq!(asset_name("linux", "x86_64").as_deref(), Some("ruppa-linux-x86_64"));
        assert_eq!(asset_name("windows", "aarch64").as_deref(), Some("ruppa-windows-aarch64.exe"));
        assert_eq!(asset_name("freebsd", "x86_64"), None);
        assert_eq!(asset_name("linux", "riscv64"), None);
    }

    #[test]
    fn checksums() {
        let sums = "abc123  ruppa-linux-x86_64\nDEF456 *ruppa-windows-x86_64.exe\n";
        assert_eq!(sum_for(sums, "ruppa-linux-x86_64").as_deref(), Some("abc123"));
        assert_eq!(sum_for(sums, "ruppa-windows-x86_64.exe").as_deref(), Some("def456"));
        assert_eq!(sum_for(sums, "ruppa-macos-universal"), None);
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(tag_version("v1.2.0"), "1.2.0");
    }
}
