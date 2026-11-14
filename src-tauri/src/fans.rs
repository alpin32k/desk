//! The fans at full speed while the CPU is hot, only when the user picked a
//! mode with fans in Options (thermal.rs decides when).
//!
//! macOS (Apple Silicon) only. desk reads the fans itself (smc.rs, no rights
//! needed); writing them needs root, so the first time the user picks a fan
//! mode, macOS asks for an admin password once (admin.rs) and desk installs
//! the `desk-fans` helper root-owned at HELPER plus a sudoers rule allowing
//! exactly `HELPER max` and `HELPER auto`.
//!
//! The fans never stay manual after desk stops driving them: desk runs
//! `auto` when it cools down, when the mode changes and when it quits; a
//! watchdog (a user shell, outliving a crash) runs it when desk is gone; and
//! a marker file left by a desk that died makes the next start run it.
//! Fans already in manual mode that desk did not set (Stats, Macs Fan Control,
//! another desk) are never touched.

use crate::fan_control::{self, Fan};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

pub const HELPER: &str = "/Library/PrivilegedHelperTools/pl.makarowski.desk.fans";
pub const SUDOERS: &str = "/etc/sudoers.d/desk-fans";
/// In desk's support dir: desk has the fans at full speed.
const MARKER: &str = "fans-forced";
/// In desk's support dir: the watchdog may stop (desk released the fans).
const STOP: &str = "fans.stop";
/// A force lost on the hardware (the Mac slept) is re-applied this often.
const RETRY: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Consent {
    Missing,
    /// Installed by a desk whose helper speaks another protocol.
    Outdated,
    Installed,
}

/// For Options: can this Mac's fans be driven, and is the permission there.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    /// Why the fan modes are greyed out, if they are.
    pub unsupported: Option<String>,
    pub consent: Consent,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FanStatus {
    /// desk has the fans at full speed.
    pub forced: bool,
    /// The fastest fan now, rounded to 100.
    pub rpm: Option<u32>,
    pub max_rpm: Option<u32>,
    /// Why desk is not driving the fans while it wants to.
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Nothing,
    Force,
    Release,
    OtherProgram,
}

pub fn consent(sudoers: bool, helper_version: Option<u32>) -> Consent {
    match (sudoers, helper_version) {
        (true, Some(v)) if v == fan_control::PROTOCOL => Consent::Installed,
        (true, Some(_)) => Consent::Outdated,
        _ => Consent::Missing,
    }
}

/// `want`: hot enough for the fans; `ours`: desk set them; `manual`: some fan
/// is manual now; `since_force`: since desk last forced them.
pub fn next_action(want: bool, ours: bool, manual: bool, since_force: Option<Duration>) -> Action {
    match (want, ours) {
        (false, true) => Action::Release,
        (false, false) => Action::Nothing,
        (true, false) if manual => Action::OtherProgram,
        (true, false) => Action::Force,
        (true, true) if !manual && since_force.is_none_or(|d| d >= RETRY) => Action::Force,
        (true, true) => Action::Nothing,
    }
}

/// The sudoers line: this user may run exactly `HELPER max` and `HELPER auto`.
pub fn sudoers_rule(user: &str) -> Result<String, String> {
    crate::admin::check_user(user)?;
    Ok(format!("{user} ALL=(root) NOPASSWD: {HELPER} max, {HELPER} auto\n"))
}

/// Root script: copy the helper into a root-only temp dir, check it is the
/// file desk hashed (nothing swapped it after the click), drop its quarantine,
/// install it and the rule (validated by visudo).
pub fn install_script(rule: &str, source: &str, sha256: &str) -> String {
    let q = crate::tmux::shell_quote;
    format!(
        "set -e; d=$(/usr/bin/mktemp -d /tmp/desk-fans.XXXXXX); trap '/bin/rm -rf \"$d\"' EXIT; \
         /bin/cp {src} \"$d/h\"; \
         [ \"$(/usr/bin/shasum -a 256 \"$d/h\" | /usr/bin/cut -d ' ' -f 1)\" = {sha} ] || \
         {{ echo 'desk-fans: checksum mismatch' >&2; exit 1; }}; \
         /usr/bin/xattr -c \"$d/h\"; \
         /usr/bin/install -d -m 0755 -o root -g wheel /Library/PrivilegedHelperTools; \
         /usr/bin/install -m 0755 -o root -g wheel \"$d/h\" {helper}; \
         printf '%s' {rule} > \"$d/s\"; /usr/sbin/visudo -cf \"$d/s\" >/dev/null; \
         /usr/bin/install -m 0440 -o root -g wheel \"$d/s\" {sudoers}",
        src = q(source),
        sha = q(sha256),
        helper = q(HELPER),
        rule = q(rule),
        sudoers = q(SUDOERS),
    )
}

pub fn uninstall_script() -> String {
    let q = crate::tmux::shell_quote;
    format!("/bin/rm -f {} {}", q(HELPER), q(SUDOERS))
}

/// Shell loop as the user: waits until desk exits or `stop` appears; if desk
/// is gone, the fans go back to auto and the marker goes.
pub fn watchdog_script(pid: u32, stop: &Path, marker: &Path) -> String {
    let q = |p: &Path| crate::tmux::shell_quote(&p.display().to_string());
    format!(
        "while /bin/kill -0 {pid} 2>/dev/null && [ ! -e {stop} ]; do /bin/sleep 5; done; \
         /bin/kill -0 {pid} 2>/dev/null || {{ /usr/bin/sudo -n {HELPER} auto; /bin/rm -f {marker}; }}",
        stop = q(stop),
        marker = q(marker),
    )
}

fn unsupported() -> String {
    tr!("tylko macOS na Apple Silicon", "macOS on Apple Silicon only").into()
}

fn no_fans() -> String {
    tr!("ten Mac nie ma wiatraków", "this Mac has no fans").into()
}

/// desk drives the fans of one Mac: state between readings (every 5 s).
pub struct Fans {
    marker: PathBuf,
    stop: PathBuf,
    ours: bool,
    last_force: Option<Instant>,
    watchdog: bool,
    #[cfg(target_os = "macos")]
    smc: Option<crate::smc::IoSmc>,
}

impl Fans {
    pub fn new(support_dir: &Path) -> Self {
        Self {
            marker: support_dir.join(MARKER),
            stop: support_dir.join(STOP),
            ours: false,
            last_force: None,
            watchdog: false,
            #[cfg(target_os = "macos")]
            smc: None,
        }
    }

    pub fn ours(&self) -> bool {
        self.ours
    }

    /// At start: a desk that died with the fans at full speed left the marker.
    pub fn recover(&mut self) {
        if !self.marker.exists() {
            return;
        }
        if consent_now() == Consent::Installed {
            let _ = run_helper("auto");
        }
        let _ = std::fs::remove_file(&self.marker);
    }

    /// One reading: force, keep, or release the fans; `want`: hot enough.
    pub fn step(&mut self, want: bool, now: Instant) -> FanStatus {
        let fans = self.read();
        let mut problem = match &fans {
            Err(_) => Some(unsupported()),
            Ok(f) if f.is_empty() => Some(no_fans()),
            Ok(_) => None,
        };
        let manual = fans.as_ref().is_ok_and(|f| fan_control::manual(f));
        let since = self.last_force.map(|t| now.saturating_duration_since(t));
        match next_action(want && problem.is_none(), self.ours, manual, since) {
            Action::Force => {
                if let Err(e) = self.force(now) {
                    problem = Some(e);
                }
            }
            Action::Release => self.release(),
            Action::OtherProgram => {
                problem = Some(tr!("wiatrakami steruje inny program", "another program controls the fans").into())
            }
            Action::Nothing => {}
        }
        let fans = if self.ours { self.read() } else { fans };
        let fans = fans.unwrap_or_default();
        let top = |v: &dyn Fn(&Fan) -> f32| fans.iter().map(v).reduce(f32::max);
        FanStatus {
            forced: self.ours,
            rpm: top(&|f| f.rpm).map(|r| ((r / 100.0).round() * 100.0) as u32),
            max_rpm: top(&|f| f.max_rpm).map(|r| r as u32),
            problem: problem.filter(|_| want),
        }
    }

    /// The fans back to auto, if desk set them (cooled down, mode changed,
    /// permission removed, desk quitting).
    pub fn release(&mut self) {
        if !self.ours {
            return;
        }
        if run_helper("auto").is_ok() {
            let _ = std::fs::remove_file(&self.marker);
        }
        let _ = std::fs::write(&self.stop, b"stop");
        self.ours = false;
        self.watchdog = false;
        self.last_force = None;
    }

    fn force(&mut self, now: Instant) -> Result<(), String> {
        match consent_now() {
            Consent::Missing => return Err(tr!("brak zgody (Opcje → Zasilanie)", "no permission (Options → Power)").into()),
            Consent::Outdated => {
                return Err(tr!(
                    "zgoda z poprzedniej wersji desk (Opcje → Zasilanie)",
                    "permission from an older desk (Options → Power)"
                )
                .into());
            }
            Consent::Installed => {}
        }
        run_helper("max")?;
        self.ours = true;
        self.last_force = Some(now);
        let _ = std::fs::write(&self.marker, b"");
        if !self.watchdog {
            let _ = std::fs::remove_file(&self.stop);
            self.watchdog = spawn_watchdog(&self.stop, &self.marker).is_ok();
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn read(&mut self) -> Result<Vec<Fan>, String> {
        if self.smc.is_none() {
            self.smc = Some(crate::smc::IoSmc::open()?);
        }
        fan_control::read(self.smc.as_mut().expect("opened above"))
    }

    #[cfg(not(target_os = "macos"))]
    fn read(&mut self) -> Result<Vec<Fan>, String> {
        Err(unsupported())
    }
}

/// For Options.
pub fn info() -> Info {
    Info {
        unsupported: unsupported_now(),
        consent: consent_now(),
    }
}

#[cfg(target_os = "macos")]
fn unsupported_now() -> Option<String> {
    match crate::smc::IoSmc::open().and_then(|mut s| fan_control::read(&mut s)) {
        Err(_) => Some(unsupported()),
        Ok(f) if f.is_empty() => Some(no_fans()),
        Ok(_) => None,
    }
}

#[cfg(not(target_os = "macos"))]
fn unsupported_now() -> Option<String> {
    Some(unsupported())
}

fn consent_now() -> Consent {
    consent(Path::new(SUDOERS).exists(), helper_version())
}

#[cfg(target_os = "macos")]
fn helper_version() -> Option<u32> {
    let out = Command::new(HELPER).arg("version").output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

#[cfg(not(target_os = "macos"))]
fn helper_version() -> Option<u32> {
    None
}

/// `sudo -n HELPER max|auto` (the rule allows exactly these).
#[cfg(target_os = "macos")]
fn run_helper(cmd: &str) -> Result<(), String> {
    let out = Command::new("/usr/bin/sudo")
        .args(["-n", HELPER, cmd])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() { format!("desk-fans {cmd}: {}", out.status) } else { err })
    }
}

#[cfg(not(target_os = "macos"))]
fn run_helper(_cmd: &str) -> Result<(), String> {
    Err(unsupported())
}

#[cfg(unix)]
fn spawn_watchdog(stop: &Path, marker: &Path) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    let mut child = Command::new("/bin/sh")
        .args(["-c", &watchdog_script(std::process::id(), stop, marker)])
        // Its own process group: Ctrl+C in `tauri dev` must not take it along.
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(not(unix))]
fn spawn_watchdog(_stop: &Path, _marker: &Path) -> Result<(), String> {
    Err(unsupported())
}

/// The helper to install: next to desk in the app, or (debug builds) where
/// .github/build-fans.sh puts it.
fn bundled_helper() -> Option<PathBuf> {
    let next = std::env::current_exe().ok()?.parent()?.join("desk-fans");
    if next.is_file() {
        return Some(next);
    }
    let dev = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/binaries/desk-fans-aarch64-apple-darwin"));
    (cfg!(debug_assertions) && dev.is_file()).then_some(dev)
}

/// The one password prompt: install the helper and its sudoers rule.
#[cfg(target_os = "macos")]
pub fn install() -> Result<Info, String> {
    use sha2::{Digest, Sha256};
    let source =
        bundled_helper().ok_or_else(|| tr!("w tej paczce desk nie ma desk-fans", "this desk build has no desk-fans").to_string())?;
    let bytes = std::fs::read(&source).map_err(|e| e.to_string())?;
    let sha: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    let user = std::env::var("USER").map_err(|_| tr!("brak zmiennej USER", "no USER variable").to_string())?;
    let script = install_script(&sudoers_rule(&user)?, &source.display().to_string(), &sha);
    crate::admin::run_admin(
        &script,
        tr!(
            "desk: jednorazowa zgoda na sterowanie wiatrakami przy przegrzaniu (tylko „maksimum” i „auto”). Później desk nie będzie już pytać.",
            "desk: one-time permission to control the fans when hot (only “max” and “auto”). desk will not ask again."
        ),
    )?;
    Ok(info())
}

#[cfg(not(target_os = "macos"))]
pub fn install() -> Result<Info, String> {
    Err(unsupported())
}

/// Remove the helper and its rule (asks for the password). The caller
/// releases the fans first.
#[cfg(target_os = "macos")]
pub fn uninstall() -> Result<Info, String> {
    crate::admin::run_admin(
        &uninstall_script(),
        tr!("desk: usunięcie zgody na sterowanie wiatrakami.", "desk: removing the fan permission."),
    )?;
    Ok(info())
}

#[cfg(not(target_os = "macos"))]
pub fn uninstall() -> Result<Info, String> {
    Ok(info())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fan_control::PROTOCOL;

    #[test]
    fn consent_needs_rule_and_current_helper() {
        assert_eq!(consent(false, Some(PROTOCOL)), Consent::Missing);
        assert_eq!(consent(true, None), Consent::Missing);
        assert_eq!(consent(true, Some(0)), Consent::Outdated);
        assert_eq!(consent(true, Some(PROTOCOL)), Consent::Installed);
    }

    #[test]
    fn next_action_forces_when_hot_and_releases_when_cool() {
        assert_eq!(next_action(true, false, false, None), Action::Force);
        assert_eq!(next_action(false, true, true, Some(Duration::from_secs(5))), Action::Release);
        assert_eq!(next_action(false, false, false, None), Action::Nothing);
    }

    #[test]
    fn next_action_leaves_another_programs_fans_alone() {
        assert_eq!(next_action(true, false, true, None), Action::OtherProgram);
        assert_eq!(next_action(false, false, true, None), Action::Nothing);
    }

    #[test]
    fn next_action_reapplies_a_lost_force_once_a_minute() {
        let s = Duration::from_secs;
        assert_eq!(next_action(true, true, false, Some(s(30))), Action::Nothing);
        assert_eq!(next_action(true, true, false, Some(s(60))), Action::Force);
        assert_eq!(next_action(true, true, true, Some(s(600))), Action::Nothing);
    }

    #[test]
    fn sudoers_rule_allows_exactly_max_and_auto() {
        assert_eq!(
            sudoers_rule("kamil").unwrap(),
            "kamil ALL=(root) NOPASSWD: /Library/PrivilegedHelperTools/pl.makarowski.desk.fans max, \
             /Library/PrivilegedHelperTools/pl.makarowski.desk.fans auto\n"
        );
        for bad in ["", "a b", "x ALL=(ALL) ALL", "-x"] {
            assert!(sudoers_rule(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn install_script_checks_the_copy_before_installing() {
        let sha = "ab12".repeat(16);
        let rule = sudoers_rule("kamil").unwrap();
        let s = install_script(&rule, "/Apps/my desk.app/Contents/MacOS/desk-fans", &sha);
        for part in [
            "'/Apps/my desk.app/Contents/MacOS/desk-fans'",
            "shasum -a 256",
            sha.as_str(),
            "xattr -c",
            "install -m 0755 -o root -g wheel",
            "'/Library/PrivilegedHelperTools/pl.makarowski.desk.fans'",
            "visudo -cf",
            "install -m 0440 -o root -g wheel",
            "'/etc/sudoers.d/desk-fans'",
        ] {
            assert!(s.contains(part), "{part} missing in {s}");
        }
        assert!(s.find("shasum -a 256").unwrap() < s.find("install -m 0755").unwrap());
    }

    #[test]
    fn uninstall_script_removes_both() {
        assert_eq!(
            uninstall_script(),
            "/bin/rm -f '/Library/PrivilegedHelperTools/pl.makarowski.desk.fans' '/etc/sudoers.d/desk-fans'"
        );
    }

    #[test]
    fn watchdog_restores_auto_only_when_desk_is_gone() {
        let dir = Path::new("/Users/k/Library/Application Support/desk");
        let s = watchdog_script(42, &dir.join("fans.stop"), &dir.join("fans-forced"));
        assert!(s.contains("/bin/kill -0 42"));
        assert!(s.contains("[ ! -e '/Users/k/Library/Application Support/desk/fans.stop' ]"));
        assert!(s.ends_with(
            "/bin/kill -0 42 2>/dev/null || { /usr/bin/sudo -n /Library/PrivilegedHelperTools/pl.makarowski.desk.fans auto; \
             /bin/rm -f '/Users/k/Library/Application Support/desk/fans-forced'; }"
        ));
    }

    /// A syntax error would only show behind the user's password prompt.
    #[cfg(unix)]
    #[test]
    fn scripts_are_valid_sh() {
        let dir = Path::new("/Users/k/Library/Application Support/desk");
        let rule = sudoers_rule("kamil").unwrap();
        for s in [
            install_script(&rule, "/Apps/my desk.app/Contents/MacOS/desk-fans", &"ab12".repeat(16)),
            uninstall_script(),
            watchdog_script(42, &dir.join("fans.stop"), &dir.join("fans-forced")),
        ] {
            let ok = Command::new("/bin/sh").args(["-n", "-c", &s]).status().unwrap().success();
            assert!(ok, "{s}");
        }
    }

    #[test]
    fn recover_without_marker_does_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let mut f = Fans::new(tmp.path());
        f.recover();
        assert!(!f.ours());
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
    }
}
