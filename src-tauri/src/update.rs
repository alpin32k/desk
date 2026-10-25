//! Updates from the project's GitHub releases. The repository is private, so
//! desk asks the GitHub CLI (`gh`, signed in as you) instead of the public
//! API; without `gh` there is simply no update notice.
//!
//! Installing: macOS swaps the app bundle for the one in the new `.dmg` and
//! starts again (tmux sessions keep running); Windows runs the new installer;
//! an AppImage is replaced in place. Other installs (`.deb`) get the release
//! page.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const REPO: &str = "knehrybecki/desk";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    /// "0.1.2".
    pub version: String,
    pub url: String,
    /// desk can install it itself (else the release page opens).
    pub installable: bool,
}

/// `vX.Y.Z` -> (X, Y, Z).
pub fn parse_version(tag: &str) -> Option<(u64, u64, u64)> {
    let mut it = tag.strip_prefix('v').unwrap_or(tag).split('.');
    let v = (it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?);
    it.next().is_none().then_some(v)
}

/// The asset this install can use, if desk can install it itself.
pub fn asset_for(assets: &[String], appimage: bool) -> Option<&String> {
    let want = |a: &&String| {
        if cfg!(target_os = "macos") {
            a.ends_with("_aarch64.dmg")
        } else if cfg!(windows) {
            a.ends_with("-setup.exe")
        } else {
            appimage && a.ends_with(".AppImage")
        }
    };
    assets.iter().find(want)
}

fn gh(args: &[&str]) -> Result<String, String> {
    let out = Command::new("gh")
        .args(args)
        .output()
        .map_err(|_| "brak gh (GitHub CLI)".to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

struct Latest {
    tag: String,
    url: String,
    assets: Vec<String>,
}

fn latest() -> Result<Latest, String> {
    let out = gh(&[
        "api",
        &format!("repos/{REPO}/releases/latest"),
        "--jq",
        "[.tag_name, .html_url, (.assets | map(.name) | join(\"\\n\"))] | join(\"\\n\")",
    ])?;
    let mut lines = out.lines();
    let tag = lines.next().unwrap_or_default().to_string();
    let url = lines.next().unwrap_or_default().to_string();
    let assets = lines.filter(|l| !l.is_empty()).map(str::to_string).collect();
    Ok(Latest { tag, url, assets })
}

/// A newer release than this desk, if there is one.
pub fn check() -> Result<Option<Update>, String> {
    let latest = latest()?;
    let (Some(new), Some(current)) = (parse_version(&latest.tag), parse_version(env!("CARGO_PKG_VERSION"))) else {
        return Ok(None);
    };
    if new <= current {
        return Ok(None);
    }
    Ok(Some(Update {
        version: format!("{}.{}.{}", new.0, new.1, new.2),
        url: latest.url,
        installable: asset_for(&latest.assets, appimage().is_some()).is_some() && target().is_ok(),
    }))
}

/// The running AppImage file (Linux).
fn appimage() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_file())
}

/// What gets replaced: the `.app` bundle (macOS), the AppImage (Linux), or
/// nothing on Windows (the installer knows where desk lives).
fn target() -> Result<PathBuf, String> {
    if cfg!(windows) {
        return std::env::current_exe().map_err(|e| e.to_string());
    }
    if cfg!(target_os = "macos") {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        return exe
            .ancestors()
            .find(|p| p.extension().is_some_and(|e| e == "app"))
            .map(Path::to_path_buf)
            .ok_or_else(|| "desk nie działa z pakietu .app".to_string());
    }
    appimage().ok_or_else(|| "desk nie działa z AppImage".to_string())
}

/// Download `version` and put it in place. Returns what to start once desk
/// has quit (the new app, or the installer).
pub fn install(version: &str) -> Result<Vec<String>, String> {
    if parse_version(version).is_none() {
        return Err(format!("zła wersja {version}"));
    }
    let latest = latest()?;
    let tag = format!("v{version}");
    if latest.tag != tag {
        return Err(format!("najnowsza wersja to {}", latest.tag));
    }
    let asset = asset_for(&latest.assets, appimage().is_some())
        .ok_or("w tym release nie ma paczki dla tego systemu")?
        .clone();
    let target = target()?;
    let dir = std::env::temp_dir().join(format!("desk-update-{version}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dir_s = dir.to_string_lossy().to_string();
    gh(&["release", "download", &tag, "--repo", REPO, "--pattern", &asset, "--dir", &dir_s, "--clobber"])?;
    let file = dir.join(&asset);
    if !file.is_file() {
        return Err(format!("nie pobrałem {asset}"));
    }
    if cfg!(target_os = "macos") {
        replace_bundle(&file, &dir, &target)?;
        Ok(vec!["open".into(), target.to_string_lossy().into()])
    } else if cfg!(windows) {
        // Passive install (progress only), then desk is started again by the user.
        Ok(vec![file.to_string_lossy().into(), "/P".into()])
    } else {
        replace_file(&file, &target)?;
        Ok(vec![target.to_string_lossy().into()])
    }
}

/// macOS: mount the dmg, copy its `desk.app` next to the running one, then
/// swap them (the running process keeps its files until it quits).
fn replace_bundle(dmg: &Path, dir: &Path, bundle: &Path) -> Result<(), String> {
    let mnt = dir.join("mnt");
    let run = |cmd: &str, args: &[&std::ffi::OsStr]| -> Result<(), String> {
        let out = Command::new(cmd).args(args).output().map_err(|e| format!("{cmd}: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(format!("{cmd}: {}", String::from_utf8_lossy(&out.stderr).trim()))
        }
    };
    run(
        "hdiutil",
        &["attach".as_ref(), "-nobrowse".as_ref(), "-readonly".as_ref(), "-mountpoint".as_ref(), mnt.as_os_str(), dmg.as_os_str()],
    )?;
    let new_app = mnt.join("desk.app");
    let staged = bundle.with_extension("app.new");
    let old = bundle.with_extension("app.old");
    let _ = std::fs::remove_dir_all(&staged);
    let _ = std::fs::remove_dir_all(&old);
    let copied = run("ditto", &[new_app.as_os_str(), staged.as_os_str()]);
    let _ = run("hdiutil", &["detach".as_ref(), "-quiet".as_ref(), mnt.as_os_str()]);
    copied?;
    std::fs::rename(bundle, &old).map_err(|e| format!("{}: {e}", bundle.display()))?;
    if let Err(e) = std::fs::rename(&staged, bundle) {
        let _ = std::fs::rename(&old, bundle);
        return Err(format!("{}: {e}", bundle.display()));
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

/// Linux AppImage: the new file takes the old one's place, executable.
fn replace_file(new: &Path, target: &Path) -> Result<(), String> {
    let staged = target.with_extension("new");
    std::fs::copy(new, &staged).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&staged, target).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_release_tags() {
        assert_eq!(parse_version("v0.1.2"), Some((0, 1, 2)));
        assert_eq!(parse_version("1.10.0"), Some((1, 10, 0)));
        assert!(parse_version("v0.1.10") > parse_version("v0.1.9"));
        assert_eq!(parse_version("v0.1"), None);
        assert_eq!(parse_version("v0.1.2-beta"), None);
        assert_eq!(parse_version("v0.1.2.3"), None);
    }

    #[test]
    fn picks_the_package_for_this_system() {
        let assets: Vec<String> = [
            "desk_0.1.2_aarch64.dmg",
            "desk_0.1.2_amd64.AppImage",
            "desk_0.1.2_amd64.deb",
            "desk_0.1.2_x64-setup.exe",
        ]
        .map(String::from)
        .to_vec();
        let want = if cfg!(target_os = "macos") {
            Some("desk_0.1.2_aarch64.dmg")
        } else if cfg!(windows) {
            Some("desk_0.1.2_x64-setup.exe")
        } else {
            Some("desk_0.1.2_amd64.AppImage")
        };
        assert_eq!(asset_for(&assets, true).map(String::as_str), want);
        if cfg!(target_os = "linux") {
            assert_eq!(asset_for(&assets, false), None, ".deb installs get the release page");
        }
    }
}
