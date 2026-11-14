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
#[cfg(unix)]
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
/// is manual now; `since_attempt`: since desk last tried `max`, worked or not.
/// Tries are at least RETRY apart: a failing helper is not run every reading,
/// and a force lost on the hardware (sleep) comes back within a minute.
pub fn next_action(want: bool, ours: bool, manual: bool, since_attempt: Option<Duration>) -> Action {
    let due = since_attempt.is_none_or(|d| d >= RETRY);
    match (want, ours) {
        (false, true) => Action::Release,
        (false, false) => Action::Nothing,
        (true, false) if manual => Action::OtherProgram,
        (true, false) if due => Action::Force,
        (true, true) if !manual && due => Action::Force,
        (true, _) => Action::Nothing,
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

/// Root script: if desk has the fans (its marker), put them back to auto
/// first (as root, no rule needed), then remove the helper and the rule.
pub fn uninstall_script(marker: &Path) -> String {
    let q = crate::tmux::shell_quote;
    let marker = q(&marker.display().to_string());
    format!(
        "if [ -e {marker} ]; then {helper} auto && /bin/rm -f {marker}; fi; /bin/rm -f {helper} {sudoers}",
        helper = q(HELPER),
        sudoers = q(SUDOERS),
    )
}

/// Shell loop as the user: waits until desk exits or `stop` appears; if desk
/// is gone, the fans go back to auto and the marker goes.
pub fn watchdog_script(pid: u32, stop: &Path, marker: &Path) -> String {
    let q = |p: &Path| crate::tmux::shell_quote(&p.display().to_string());
    format!(
        "while /bin/kill -0 {pid} 2>/dev/null && [ ! -e {stop} ]; do /bin/sleep 5; done; \
         /bin/kill -0 {pid} 2>/dev/null || {{ /usr/bin/sudo -n {HELPER} auto && /bin/rm -f {marker}; }}",
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

pub type ReadFans = Box<dyn FnMut() -> Result<Vec<Fan>, String> + Send>;
pub type RunHelper = Box<dyn FnMut(&str) -> Result<(), String> + Send>;
pub type StartWatchdog = Box<dyn FnMut(&Path, &Path) -> Result<(), String> + Send>;

/// What `Fans` does to the system, so tests can stand in for the SMC, sudo
/// and the watchdog.
pub struct Ops {
    pub read: ReadFans,
    /// `sudo -n HELPER max|auto`.
    pub helper: RunHelper,
    pub consent: Box<dyn FnMut() -> Consent + Send>,
    /// Start the watchdog (stop file, marker).
    pub watchdog: StartWatchdog,
}

impl Ops {
    pub fn system() -> Ops {
        #[cfg(target_os = "macos")]
        let read: ReadFans = {
            let mut smc: Option<crate::smc::IoSmc> = None;
            Box::new(move || {
                if smc.is_none() {
                    smc = Some(crate::smc::IoSmc::open()?);
                }
                fan_control::read(smc.as_mut().expect("opened above"))
            })
        };
        #[cfg(not(target_os = "macos"))]
        let read: ReadFans = Box::new(|| Err(unsupported()));
        Ops {
            read,
            helper: Box::new(run_helper),
            consent: Box::new(consent_now),
            watchdog: Box::new(spawn_watchdog),
        }
    }
}

/// desk drives the fans of one Mac: state between readings (every 5 s).
pub struct Fans {
    marker: PathBuf,
    stop: PathBuf,
    /// desk set the fans (or a failed try of its own may have left them manual).
    ours: bool,
    /// Removing the permission: no forcing until it is done.
    paused: bool,
    watchdog: bool,
    /// The last `max` tried, worked or not: tries are a minute apart.
    last_attempt: Option<Instant>,
    /// Why the last try failed, kept while backing off.
    problem: Option<String>,
    ops: Ops,
}

impl Fans {
    pub fn new(support_dir: &Path) -> Self {
        Self::with_ops(support_dir, Ops::system())
    }

    pub fn with_ops(support_dir: &Path, ops: Ops) -> Self {
        Self {
            marker: support_dir.join(MARKER),
            stop: support_dir.join(STOP),
            ours: false,
            paused: false,
            watchdog: false,
            last_attempt: None,
            problem: None,
            ops,
        }
    }

    pub fn ours(&self) -> bool {
        self.ours
    }

    /// At start: a desk that died with the fans at full speed left the marker.
    /// If `auto` cannot run now, the fans count as desk's, so it is retried.
    pub fn recover(&mut self) {
        if !self.marker.exists() {
            return;
        }
        // An outdated rule still allows `auto`.
        if (self.ops.consent)() != Consent::Missing && (self.ops.helper)("auto").is_err() {
            self.ours = true;
            return;
        }
        let _ = std::fs::remove_file(&self.marker);
    }

    /// Before removing the permission: the fans back to auto, and no forcing
    /// until `resume`.
    pub fn pause(&mut self) {
        self.release();
        self.paused = true;
    }

    pub fn resume(&mut self) {
        self.paused = false;
    }

    /// One reading: force, keep, or release the fans; `want`: hot enough.
    pub fn step(&mut self, want: bool, now: Instant) -> FanStatus {
        let want = want && !self.paused;
        if !want {
            self.problem = None;
        }
        let fans = (self.ops.read)();
        let mut problem = match &fans {
            Err(_) => Some(unsupported()),
            Ok(f) if f.is_empty() => Some(no_fans()),
            Ok(_) => None,
        };
        let manual = fans.as_ref().is_ok_and(|f| fan_control::manual(f));
        let since = self.last_attempt.map(|t| now.saturating_duration_since(t));
        match next_action(want && problem.is_none(), self.ours, manual, since) {
            Action::Force => self.force(now),
            Action::Release => self.release(),
            Action::OtherProgram => {
                self.problem = Some(tr!("wiatrakami steruje inny program", "another program controls the fans").into())
            }
            Action::Nothing => {}
        }
        problem = problem.or_else(|| self.problem.clone());
        let fans = if self.ours { (self.ops.read)() } else { fans };
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
    /// permission removed, desk quitting). If `auto` fails they stay desk's:
    /// the marker and the watchdog stay, and the next reading tries again.
    pub fn release(&mut self) {
        if !self.ours {
            return;
        }
        match (self.ops.helper)("auto") {
            Ok(()) => {
                self.ours = false;
                self.last_attempt = None;
                self.forget();
            }
            Err(e) => self.problem = Some(e),
        }
    }

    fn force(&mut self, now: Instant) {
        self.last_attempt = Some(now);
        match (self.ops.consent)() {
            Consent::Missing => {
                self.problem = Some(tr!("brak zgody (Opcje → Zasilanie)", "no permission (Options → Power)").into());
                return;
            }
            Consent::Outdated => {
                self.problem = Some(
                    tr!(
                        "zgoda z poprzedniej wersji desk (Opcje → Zasilanie)",
                        "permission from an older desk (Options → Power)"
                    )
                    .into(),
                );
                return;
            }
            Consent::Installed => {}
        }
        // Marked first: a desk that dies during `max` must still lead back to auto.
        let _ = std::fs::write(&self.marker, b"");
        if !self.watchdog {
            let _ = std::fs::remove_file(&self.stop);
            self.watchdog = (self.ops.watchdog)(&self.stop, &self.marker).is_ok();
        }
        match (self.ops.helper)("max") {
            Ok(()) => {
                self.ours = true;
                self.problem = None;
            }
            Err(e) => {
                self.problem = Some(e);
                // The helper rolls back to auto; a fan still manual is desk's to put back.
                self.ours = (self.ops.read)().is_ok_and(|f| fan_control::manual(&f));
                if !self.ours {
                    self.forget();
                }
            }
        }
    }

    /// No more marker; the watchdog may stop.
    fn forget(&mut self) {
        let _ = std::fs::remove_file(&self.marker);
        let _ = std::fs::write(&self.stop, b"stop");
        self.watchdog = false;
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
#[cfg(target_os = "macos")]
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
/// pauses the fans first; desk's marker is in `support_dir`.
#[cfg(target_os = "macos")]
pub fn uninstall(support_dir: &Path) -> Result<Info, String> {
    crate::admin::run_admin(
        &uninstall_script(&support_dir.join(MARKER)),
        tr!("desk: usunięcie zgody na sterowanie wiatrakami.", "desk: removing the fan permission."),
    )?;
    Ok(info())
}

#[cfg(not(target_os = "macos"))]
pub fn uninstall(_support_dir: &Path) -> Result<Info, String> {
    Ok(info())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fan_control::PROTOCOL;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

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
    fn next_action_backs_off_after_a_failed_force() {
        let s = Duration::from_secs;
        assert_eq!(next_action(true, false, false, Some(s(30))), Action::Nothing);
        assert_eq!(next_action(true, false, false, Some(s(60))), Action::Force);
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
    fn uninstall_script_puts_desks_fans_back_first() {
        let marker = Path::new("/Users/k/Library/Application Support/desk/fans-forced");
        assert_eq!(
            uninstall_script(marker),
            "if [ -e '/Users/k/Library/Application Support/desk/fans-forced' ]; then \
             '/Library/PrivilegedHelperTools/pl.makarowski.desk.fans' auto && \
             /bin/rm -f '/Users/k/Library/Application Support/desk/fans-forced'; fi; \
             /bin/rm -f '/Library/PrivilegedHelperTools/pl.makarowski.desk.fans' '/etc/sudoers.d/desk-fans'"
        );
    }

    #[test]
    fn watchdog_restores_auto_only_when_desk_is_gone() {
        // Whole paths, not joined: Windows would join them with a backslash.
        let s = watchdog_script(
            42,
            Path::new("/Users/k/Library/Application Support/desk/fans.stop"),
            Path::new("/Users/k/Library/Application Support/desk/fans-forced"),
        );
        assert!(s.contains("/bin/kill -0 42"));
        assert!(s.contains("[ ! -e '/Users/k/Library/Application Support/desk/fans.stop' ]"));
        assert!(s.ends_with(
            "/bin/kill -0 42 2>/dev/null || { /usr/bin/sudo -n /Library/PrivilegedHelperTools/pl.makarowski.desk.fans auto && \
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
            uninstall_script(&dir.join("fans-forced")),
            watchdog_script(42, &dir.join("fans.stop"), &dir.join("fans-forced")),
        ] {
            let ok = Command::new("/bin/sh").args(["-n", "-c", &s]).status().unwrap().success();
            assert!(ok, "{s}");
        }
    }

    /// Fake system: two fans, a helper that logs its commands (and fails
    /// those listed in `fail`), the given consent.
    struct Fake {
        log: Arc<Mutex<Vec<String>>>,
        manual: Arc<AtomicBool>,
        fail: Arc<Mutex<Vec<&'static str>>>,
        /// A failing `max` leaves the fans manual (its rollback failed too).
        half: Arc<AtomicBool>,
    }

    fn fake(dir: &Path, consent: Consent) -> (Fans, Fake) {
        let f = Fake {
            log: Arc::default(),
            manual: Arc::default(),
            fail: Arc::default(),
            half: Arc::default(),
        };
        let (manual, log, fail) = (f.manual.clone(), f.log.clone(), f.fail.clone());
        let read = Box::new(move || {
            let mode = if manual.load(Ordering::SeqCst) { fan_control::MANUAL } else { fan_control::AUTO };
            Ok(vec![Fan { mode, rpm: 2300.0, max_rpm: 6800.0 }; 2])
        });
        let (manual, marker, half) = (f.manual.clone(), dir.join(MARKER), f.half.clone());
        let helper = Box::new(move |cmd: &str| {
            let marked = if marker.exists() { "+marker" } else { "" };
            log.lock().unwrap().push(format!("{cmd}{marked}"));
            if fail.lock().unwrap().contains(&cmd) {
                if cmd == "max" && half.load(Ordering::SeqCst) {
                    manual.store(true, Ordering::SeqCst);
                }
                return Err(format!("{cmd} failed"));
            }
            manual.store(cmd == "max", Ordering::SeqCst);
            Ok(())
        });
        let log = f.log.clone();
        let watchdog = Box::new(move |_: &Path, _: &Path| {
            log.lock().unwrap().push("watchdog".into());
            Ok(())
        });
        let ops = Ops {
            read,
            helper,
            consent: Box::new(move || consent),
            watchdog,
        };
        (Fans::with_ops(dir, ops), f)
    }

    fn log(f: &Fake) -> Vec<String> {
        f.log.lock().unwrap().clone()
    }

    #[test]
    fn recover_without_marker_does_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut fans, f) = fake(tmp.path(), Consent::Installed);
        fans.recover();
        assert!(!fans.ours());
        assert!(log(&f).is_empty());
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
    }

    #[test]
    fn recover_runs_auto_with_an_outdated_permission() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(MARKER), b"").unwrap();
        let (mut fans, f) = fake(tmp.path(), Consent::Outdated);
        fans.recover();
        assert_eq!(log(&f), ["auto+marker"]);
        assert!(!tmp.path().join(MARKER).exists());
    }

    #[test]
    fn marker_and_watchdog_come_before_max() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut fans, f) = fake(tmp.path(), Consent::Installed);
        let s = fans.step(true, Instant::now());
        assert!(s.forced);
        assert_eq!(log(&f), ["watchdog", "max+marker"]);
    }

    #[test]
    fn paused_fans_never_force() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut fans, f) = fake(tmp.path(), Consent::Installed);
        fans.pause();
        let s = fans.step(true, Instant::now());
        assert!(!s.forced);
        assert_eq!(s.problem, None);
        assert!(log(&f).is_empty());
        fans.resume();
        assert!(fans.step(true, Instant::now()).forced);
    }

    #[test]
    fn pausing_releases_desks_fans() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut fans, f) = fake(tmp.path(), Consent::Installed);
        let t = Instant::now();
        fans.step(true, t);
        fans.pause();
        assert!(!fans.ours());
        assert_eq!(log(&f).last().unwrap(), "auto+marker");
        assert!(!fans.step(true, t + Duration::from_secs(120)).forced);
    }

    #[test]
    fn a_failed_max_backs_off_for_a_minute() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut fans, f) = fake(tmp.path(), Consent::Installed);
        f.fail.lock().unwrap().push("max");
        let t = Instant::now();
        assert_eq!(fans.step(true, t).problem.as_deref(), Some("max failed"));
        // The same problem while backing off, and no second try.
        assert_eq!(fans.step(true, t + Duration::from_secs(5)).problem.as_deref(), Some("max failed"));
        assert_eq!(log(&f).iter().filter(|c| c.starts_with("max")).count(), 1);
        fans.step(true, t + Duration::from_secs(60));
        assert_eq!(log(&f).iter().filter(|c| c.starts_with("max")).count(), 2);
    }

    #[test]
    fn a_failed_max_that_left_the_fans_in_auto_cleans_up() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut fans, f) = fake(tmp.path(), Consent::Installed);
        f.fail.lock().unwrap().push("max");
        fans.step(true, Instant::now());
        assert!(!fans.ours());
        assert!(!tmp.path().join(MARKER).exists());
        assert!(tmp.path().join(STOP).exists());
    }

    #[test]
    fn a_failed_max_that_left_a_fan_manual_is_still_desks() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut fans, f) = fake(tmp.path(), Consent::Installed);
        f.fail.lock().unwrap().push("max");
        f.half.store(true, Ordering::SeqCst); // the helper's own rollback did not get through
        let t = Instant::now();
        assert_eq!(fans.step(true, t).problem.as_deref(), Some("max failed"));
        assert!(fans.ours());
        assert!(tmp.path().join(MARKER).exists());
        fans.step(false, t + Duration::from_secs(90));
        assert_eq!(log(&f).last().unwrap(), "auto+marker");
        assert!(!fans.ours());
    }

    #[test]
    fn a_failed_auto_keeps_the_fans_desks_and_retries() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut fans, f) = fake(tmp.path(), Consent::Installed);
        let t = Instant::now();
        fans.step(true, t);
        f.fail.lock().unwrap().push("auto");
        fans.step(false, t + Duration::from_secs(70));
        assert!(fans.ours());
        assert!(tmp.path().join(MARKER).exists());
        assert!(!tmp.path().join(STOP).exists());
        f.fail.lock().unwrap().clear();
        fans.step(false, t + Duration::from_secs(75));
        assert!(!fans.ours());
        assert_eq!(log(&f).iter().filter(|c| c.starts_with("auto")).count(), 2);
    }

    #[test]
    fn another_programs_manual_fans_are_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut fans, f) = fake(tmp.path(), Consent::Installed);
        f.manual.store(true, Ordering::SeqCst);
        let s = fans.step(true, Instant::now());
        assert!(!s.forced);
        assert!(s.problem.is_some());
        fans.step(false, Instant::now());
        assert!(log(&f).is_empty());
    }
}
