//! "Coffee": keep the computer awake while desk runs, optionally even with the
//! lid closed, with Amphetamine-style safeguards against overheating and an
//! empty battery.
//!
//! macOS
//! - `Awake`: `caffeinate -dims -w <desk pid>` (no admin rights needed).
//! - `Lid`: additionally `pmset -a disablesleep 1`. The first time, macOS asks
//!   for an admin password once to install `/etc/sudoers.d/desk-pmset`, which
//!   allows exactly `pmset -a disablesleep 1` and `... 0` without a password
//!   (like Amphetamine Enhancer). A watchdog process restores sleep as soon as
//!   desk exits or coffee is turned off, even if desk crashes.
//!
//! Linux
//! - `systemd-inhibit --what=idle:sleep[:handle-lid-switch]` held until desk exits.
//!
//! Every 20 s the guard checks temperature and battery and turns coffee off
//! when the machine is too hot or the battery is low.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub const LOW_BATTERY_PERCENT: u8 = 15;
/// Linux: any thermal zone at or above this (millidegrees C) counts as too hot.
pub const HOT_MILLI_C: i64 = 90_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Off,
    Awake,
    Lid,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub level: Level,
    /// macOS: the one-time sudoers rule for lid mode is installed.
    pub helper_installed: bool,
    /// `pmset` SleepDisabled is on (also true when left over from a crash or reboot).
    pub sleep_disabled: bool,
    /// Why coffee was last turned off automatically, if it was.
    pub stopped_because: Option<String>,
}

pub struct Coffee {
    level: Mutex<Level>,
    /// caffeinate (macOS) or systemd-inhibit (Linux), tied to desk's lifetime.
    keepawake: Mutex<Option<Child>>,
    stop_file: PathBuf,
    stopped_because: Mutex<Option<String>>,
}

impl Coffee {
    pub fn new(support_dir: &Path) -> Self {
        Self {
            level: Mutex::new(Level::Off),
            keepawake: Mutex::new(None),
            stop_file: support_dir.join("coffee.stop"),
            stopped_because: Mutex::new(None),
        }
    }

    pub fn level(&self) -> Level {
        *self.level.lock().unwrap()
    }

    pub fn status(&self) -> Status {
        Status {
            level: *self.level.lock().unwrap(),
            helper_installed: cfg!(target_os = "macos") && helper_installed(),
            sleep_disabled: sleep_disabled(),
            stopped_because: self.stopped_because.lock().unwrap().clone(),
        }
    }

    pub fn set(&self, level: Level) -> Result<Status, String> {
        let current = *self.level.lock().unwrap();
        if level == current {
            return Ok(self.status());
        }
        if level != Level::Off
            && let Some(reason) = guard_reason(thermal_hot(), battery())
        {
            return Err(format!("Nie włączam: {reason}"));
        }
        #[cfg(target_os = "macos")]
        {
            if level == Level::Lid {
                if !helper_installed() {
                    install_helper()?; // the only password prompt
                }
                pmset_disablesleep(true)?;
                let _ = std::fs::remove_file(&self.stop_file);
                spawn_watchdog(&self.stop_file)?;
            } else if current == Level::Lid {
                // The watchdog sees this file and also restores sleep.
                std::fs::write(&self.stop_file, b"stop").map_err(|e| e.to_string())?;
                let _ = pmset_disablesleep(false);
            }
        }
        if let Some(mut c) = self.keepawake.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if level != Level::Off {
            let child = keepawake_command(level)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("{e}"))?;
            *self.keepawake.lock().unwrap() = Some(child);
        }
        *self.level.lock().unwrap() = level;
        *self.stopped_because.lock().unwrap() = None;
        Ok(self.status())
    }

    /// Restore normal sleep when sleep is disabled but no watchdog of ours is
    /// running (left over from a reboot or a killed watchdog).
    pub fn restore_sleep(&self) -> Result<Status, String> {
        #[cfg(target_os = "macos")]
        {
            if helper_installed() {
                pmset_disablesleep(false)?;
            } else {
                run_admin("/usr/bin/pmset -a disablesleep 0")?;
            }
        }
        Ok(self.status())
    }

    /// Remove the sudoers rule (asks for the admin password).
    pub fn uninstall_helper(&self) -> Result<Status, String> {
        #[cfg(target_os = "macos")]
        {
            let _ = self.set(Level::Off);
            run_admin(&format!("/bin/rm -f {}", crate::tmux::shell_quote(SUDOERS)))?;
        }
        Ok(self.status())
    }

    /// Turn everything off; used by the guard and on app exit.
    pub fn shutdown(&self, reason: Option<String>) {
        let _ = self.set(Level::Off);
        *self.stopped_because.lock().unwrap() = reason;
    }
}

/// Background check every 20 s while coffee is on.
pub fn spawn_guard(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(20));
            let coffee = app.state::<crate::AppCoffee>();
            if coffee.0.level() == Level::Off {
                continue;
            }
            if let Some(reason) = guard_reason(thermal_hot(), battery()) {
                coffee.0.shutdown(Some(reason));
                let _ = app.emit("coffee", coffee.0.status());
            }
        }
    });
}

/// Why coffee must stop, if it must: the system reports high thermal
/// pressure, or the battery is low and discharging.
pub fn guard_reason(hot: bool, battery: Option<Battery>) -> Option<String> {
    if hot {
        return Some("komputer jest za gorący".into());
    }
    match battery {
        Some(b) if !b.charging && b.percent <= LOW_BATTERY_PERCENT => {
            Some(format!("bateria {}%", b.percent))
        }
        _ => None,
    }
}

fn keepawake_command(level: Level) -> Command {
    let pid = std::process::id().to_string();
    if cfg!(target_os = "macos") {
        let mut c = Command::new("/usr/bin/caffeinate");
        c.args(["-dims", "-w", &pid]);
        c
    } else {
        let what = if level == Level::Lid {
            "idle:sleep:handle-lid-switch"
        } else {
            "idle:sleep"
        };
        let mut c = Command::new("systemd-inhibit");
        c.args([
            &format!("--what={what}"),
            "--who=desk",
            "--why=Claude Code sessions are running",
            "--mode=block",
            "tail",
            &format!("--pid={pid}"),
            "-f",
            "/dev/null",
        ]);
        c
    }
}

/// macOS: thermal state serious or critical.
#[cfg(target_os = "macos")]
fn thermal_hot() -> bool {
    use objc2_foundation::{NSProcessInfo, NSProcessInfoThermalState};
    NSProcessInfo::processInfo().thermalState().0 >= NSProcessInfoThermalState::Serious.0
}

/// Linux: the hottest thermal zone.
#[cfg(not(target_os = "macos"))]
fn thermal_hot() -> bool {
    let temps = std::fs::read_dir("/sys/class/thermal")
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("thermal_zone"))
        .filter_map(|e| std::fs::read_to_string(e.path().join("temp")).ok())
        .filter_map(|t| t.trim().parse::<i64>().ok());
    too_hot(temps)
}

pub fn too_hot(milli_c: impl IntoIterator<Item = i64>) -> bool {
    milli_c.into_iter().any(|t| t >= HOT_MILLI_C)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
}

#[cfg(target_os = "macos")]
fn battery() -> Option<Battery> {
    let out = Command::new("/usr/bin/pmset")
        .args(["-g", "batt"])
        .output()
        .ok()?;
    parse_battery(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(not(target_os = "macos"))]
fn battery() -> Option<Battery> {
    let dir = std::fs::read_dir("/sys/class/power_supply")
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().starts_with("BAT"))?
        .path();
    let capacity = std::fs::read_to_string(dir.join("capacity")).ok()?;
    let status = std::fs::read_to_string(dir.join("status")).unwrap_or_default();
    sysfs_battery(&capacity, &status)
}

/// Linux sysfs: `capacity` is a percentage, `status` Charging/Discharging/Full/Not charging.
pub fn sysfs_battery(capacity: &str, status: &str) -> Option<Battery> {
    Some(Battery {
        percent: capacity.trim().parse().ok()?,
        charging: status.trim() != "Discharging",
    })
}

/// `pmset -g batt`: "... 'AC Power'\n -InternalBattery-0 (id=1)\t64%; discharging; ..."
pub fn parse_battery(out: &str) -> Option<Battery> {
    let line = out.lines().find(|l| l.contains("InternalBattery"))?;
    let after_tab = line.split('\t').nth(1)?;
    let mut fields = after_tab.split(';').map(str::trim);
    let percent = fields.next()?.trim_end_matches('%').parse().ok()?;
    let state = fields.next().unwrap_or_default();
    let on_ac = out.contains("'AC Power'");
    Some(Battery {
        percent,
        charging: on_ac || state == "charging" || state == "charged",
    })
}

fn sleep_disabled() -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    Command::new("/usr/bin/pmset")
        .arg("-g")
        .output()
        .map(|o| parse_sleep_disabled(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or(false)
}

pub fn parse_sleep_disabled(out: &str) -> bool {
    out.lines().any(|l| {
        let mut f = l.split_whitespace();
        f.next() == Some("SleepDisabled") && f.next() == Some("1")
    })
}

pub const SUDOERS: &str = "/etc/sudoers.d/desk-pmset";

fn helper_installed() -> bool {
    Path::new(SUDOERS).exists()
}

/// The sudoers line: this user may run exactly these two pmset commands.
pub fn sudoers_rule(user: &str) -> Result<String, String> {
    let ok = !user.is_empty()
        && user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        && !user.starts_with('-');
    if !ok {
        return Err(format!("nieobsługiwana nazwa użytkownika: {user:?}"));
    }
    Ok(format!(
        "{user} ALL=(root) NOPASSWD: /usr/bin/pmset -a disablesleep 1, /usr/bin/pmset -a disablesleep 0\n"
    ))
}

/// Root script: write the rule to a temp file, validate it with visudo, install it 0440.
pub fn install_script(rule: &str) -> String {
    format!(
        "t=$(/usr/bin/mktemp /tmp/desk-pmset.XXXXXX) && printf '%s' {} > \"$t\" && \
         /usr/sbin/visudo -cf \"$t\" >/dev/null && \
         /usr/bin/install -m 0440 -o root -g wheel \"$t\" {}; rc=$?; /bin/rm -f \"$t\"; exit $rc",
        crate::tmux::shell_quote(rule),
        crate::tmux::shell_quote(SUDOERS)
    )
}

#[cfg(target_os = "macos")]
fn install_helper() -> Result<(), String> {
    let user = std::env::var("USER").map_err(|_| "brak zmiennej USER".to_string())?;
    run_admin(&install_script(&sudoers_rule(&user)?))
}

#[cfg(target_os = "macos")]
fn pmset_disablesleep(on: bool) -> Result<(), String> {
    let out = Command::new("/usr/bin/sudo")
        .args([
            "-n",
            "/usr/bin/pmset",
            "-a",
            "disablesleep",
            if on { "1" } else { "0" },
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Shell loop that waits until desk exits or the stop file appears, then
/// re-enables sleep. Runs as the user (sudo rule), outlives a desk crash.
pub fn watchdog_script(pid: u32, stop_file: &Path) -> String {
    let stop = crate::tmux::shell_quote(&stop_file.display().to_string());
    format!(
        "while /bin/kill -0 {pid} 2>/dev/null && [ ! -e {stop} ]; do /bin/sleep 5; done; \
         /usr/bin/sudo -n /usr/bin/pmset -a disablesleep 0"
    )
}

#[cfg(target_os = "macos")]
fn spawn_watchdog(stop_file: &Path) -> Result<(), String> {
    let mut child = Command::new("/bin/sh")
        .args(["-c", &watchdog_script(std::process::id(), stop_file)])
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

/// Run a fixed shell command as root; macOS shows its own password dialog.
#[cfg(target_os = "macos")]
fn run_admin(script: &str) -> Result<(), String> {
    let apple = format!(
        "do shell script {} with administrator privileges with prompt \"desk: jednorazowa zgoda na tryb kawy z zamkniętą klapką (pmset disablesleep). Później desk nie będzie już pytać.\"",
        applescript_quote(script)
    );
    let out = Command::new("/usr/bin/osascript")
        .args(["-e", &apple])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(if err.contains("-128") {
            "anulowano".into()
        } else {
            err.trim().to_string()
        })
    }
}

pub fn applescript_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_battery() {
        let out = "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=7733347)\t64%; discharging; 2:02 remaining present: true\n";
        assert_eq!(
            parse_battery(out),
            Some(Battery {
                percent: 64,
                charging: false
            })
        );
        let ac = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=1)\t12%; charging; 1:00 remaining present: true\n";
        assert!(parse_battery(ac).unwrap().charging);
        assert_eq!(parse_battery("Now drawing from 'AC Power'\n"), None); // desktop Mac
    }

    #[test]
    fn guard_stops_on_heat_and_low_battery() {
        let ok = Some(Battery {
            percent: 50,
            charging: false,
        });
        assert_eq!(guard_reason(false, ok), None);
        assert!(guard_reason(true, ok).unwrap().contains("gorący"));
        assert!(guard_reason(true, None).is_some());
        let low = Some(Battery {
            percent: 15,
            charging: false,
        });
        assert_eq!(guard_reason(false, low).as_deref(), Some("bateria 15%"));
        let low_charging = Some(Battery {
            percent: 5,
            charging: true,
        });
        assert_eq!(guard_reason(false, low_charging), None);
    }

    #[test]
    fn linux_sensors() {
        assert!(!too_hot([45_000, 89_999]));
        assert!(too_hot([45_000, 95_000]));
        assert!(!too_hot([]));
        assert_eq!(
            sysfs_battery("14\n", "Discharging\n"),
            Some(Battery {
                percent: 14,
                charging: false
            })
        );
        assert!(sysfs_battery("80", "Not charging").unwrap().charging);
        assert_eq!(sysfs_battery("x", "Full"), None);
    }

    #[test]
    fn parses_sleep_disabled() {
        assert!(parse_sleep_disabled(" SleepDisabled\t\t1\n sleep 1\n"));
        assert!(!parse_sleep_disabled(" SleepDisabled\t\t0\n"));
        assert!(!parse_sleep_disabled(""));
    }

    #[test]
    fn watchdog_restores_sleep_and_quotes_paths() {
        let s = watchdog_script(
            42,
            Path::new("/Users/k/Library/Application Support/desk/coffee.stop"),
        );
        assert!(s.contains("/bin/kill -0 42"));
        assert!(s.contains("[ ! -e '/Users/k/Library/Application Support/desk/coffee.stop' ]"));
        assert!(s.ends_with("/usr/bin/sudo -n /usr/bin/pmset -a disablesleep 0"));
    }

    #[test]
    fn sudoers_rule_is_minimal_and_validated() {
        let r = sudoers_rule("kamil").unwrap();
        assert_eq!(
            r,
            "kamil ALL=(root) NOPASSWD: /usr/bin/pmset -a disablesleep 1, /usr/bin/pmset -a disablesleep 0\n"
        );
        assert!(sudoers_rule("").is_err());
        assert!(sudoers_rule("a b").is_err());
        assert!(sudoers_rule("x ALL=(ALL) ALL").is_err());
        assert!(sudoers_rule("-x").is_err());
        let script = install_script(&r);
        assert!(script.contains("visudo -cf"));
        assert!(script.contains("install -m 0440 -o root -g wheel"));
        assert!(script.contains("'/etc/sudoers.d/desk-pmset'"));
    }

    /// Spawns a real keep-awake process; run with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn awake_spawns_and_off_stops_keepawake() {
        let tmp = tempfile::tempdir().unwrap();
        let c = Coffee::new(tmp.path());
        assert_eq!(c.set(Level::Awake).unwrap().level, Level::Awake);
        let pid = c.keepawake.lock().unwrap().as_ref().unwrap().id();
        assert!(
            Command::new("/bin/kill")
                .args(["-0", &pid.to_string()])
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(c.set(Level::Off).unwrap().level, Level::Off);
        assert!(c.keepawake.lock().unwrap().is_none());
        assert!(
            !Command::new("/bin/kill")
                .args(["-0", &pid.to_string()])
                .status()
                .unwrap()
                .success()
        );
    }

    #[test]
    fn applescript_strings_are_escaped() {
        assert_eq!(applescript_quote(r#"a "b" \c"#), r#""a \"b\" \\c""#);
    }
}
