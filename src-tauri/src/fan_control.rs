//! The fans through the SMC: read them, put them all at full speed, give them
//! back to macOS. Shared by desk (reading, without root) and the `desk-fans`
//! helper (writing, as root; src-tauri/fans-helper), which includes this file
//! with `#[path]`, so nothing here may use desk's other modules or `tr!`.
//!
//! Apple Silicon: writes need `Ftst` = 1 first; a fan's mode (`FnMd`) then
//! takes manual (1) after a moment, and `FnTg` is its target speed. Safety
//! rule: the target is set to the maximum BEFORE the fan goes manual, and any
//! failure ends in auto, so a fan never stays manual at a low (old) target.

pub const PROTOCOL: u32 = 1;
pub const AUTO: u8 = 0;
pub const MANUAL: u8 = 1;
/// Writes of manual mode tried after unlocking, with a pause (50 ms) between.
pub const TRIES: usize = 100;

/// Typed access to SMC keys ("FNum", "F0Md"...).
pub trait Smc {
    fn read_u8(&mut self, key: &str) -> Result<u8, String>;
    fn read_f32(&mut self, key: &str) -> Result<f32, String>;
    fn write_u8(&mut self, key: &str, value: u8) -> Result<(), String>;
    fn write_f32(&mut self, key: &str, value: f32) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fan {
    pub mode: u8,
    pub rpm: f32,
    pub max_rpm: f32,
}

/// The key of a fan's field: `key(0, "Md")` is "F0Md".
pub fn key(fan: u8, field: &str) -> String {
    format!("F{fan}{field}")
}

/// Every fan. Err: this Mac's fans cannot be controlled (no `Ftst` or
/// `FNum`); an empty list: it has none.
pub fn read(smc: &mut impl Smc) -> Result<Vec<Fan>, String> {
    smc.read_u8("Ftst")?;
    let count = smc.read_u8("FNum")?;
    (0..count)
        .map(|i| {
            Ok(Fan {
                mode: smc.read_u8(&key(i, "Md"))?,
                rpm: smc.read_f32(&key(i, "Ac"))?,
                max_rpm: smc.read_f32(&key(i, "Mx"))?,
            })
        })
        .collect()
}

/// Some fan is in manual mode (desk's or another program's doing).
pub fn manual(fans: &[Fan]) -> bool {
    fans.iter().any(|f| f.mode == MANUAL)
}

/// Every fan at full speed. On any failure the fans go back to auto first.
pub fn force_max(smc: &mut impl Smc, pause: &mut dyn FnMut()) -> Result<(), String> {
    let fans = read(smc)?;
    if fans.is_empty() {
        return Err("no fans".into());
    }
    let result = apply_max(smc, &fans, pause);
    if result.is_err() {
        let _ = restore_auto(smc);
    }
    result
}

fn apply_max(smc: &mut impl Smc, fans: &[Fan], pause: &mut dyn FnMut()) -> Result<(), String> {
    smc.write_u8("Ftst", 1)?;
    for (i, fan) in (0u8..).zip(fans) {
        smc.write_f32(&key(i, "Tg"), fan.max_rpm)?;
        let mut last = Ok(());
        for _ in 0..TRIES {
            last = smc.write_u8(&key(i, "Md"), MANUAL);
            if last.is_ok() {
                break;
            }
            pause();
        }
        last?;
    }
    for (i, fan) in (0u8..).zip(fans) {
        let mode = smc.read_u8(&key(i, "Md"))?;
        let target = smc.read_f32(&key(i, "Tg"))?;
        if mode != MANUAL || (target - fan.max_rpm).abs() >= 1.0 {
            return Err(format!("fan {i}: not applied (mode {mode}, target {target})"));
        }
    }
    Ok(())
}

/// Every fan back in auto, then the SMC locked again. Tries every fan even
/// when one fails, and returns the first error.
pub fn restore_auto(smc: &mut impl Smc) -> Result<(), String> {
    let count = smc.read_u8("FNum")?;
    let mut first: Result<(), String> = Ok(());
    for i in 0..count {
        let r = smc.write_u8(&key(i, "Md"), AUTO);
        if first.is_ok() {
            first = r;
        }
    }
    let r = smc.write_u8("Ftst", 0);
    if first.is_ok() {
        first = r;
    }
    first?;
    for i in 0..count {
        if smc.read_u8(&key(i, "Md"))? == MANUAL {
            return Err(format!("fan {i}: still manual"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    /// An SMC in memory: two fans in auto by default, writes logged.
    struct FakeSmc {
        u8s: HashMap<String, u8>,
        f32s: HashMap<String, f32>,
        /// Keys whose writes fail.
        reject: HashSet<String>,
        /// The first N writes to any `F?Md` fail (locked until Ftst settles).
        md_locked: usize,
        log: Vec<String>,
    }

    impl FakeSmc {
        fn two_fans() -> Self {
            let u8s = [("Ftst", 0), ("FNum", 2), ("F0Md", 3), ("F1Md", 3)];
            let f32s = [
                ("F0Mx", 6800.0),
                ("F1Mx", 6800.0),
                ("F0Ac", 0.0),
                ("F1Ac", 0.0),
                ("F0Tg", 0.0),
                ("F1Tg", 0.0),
            ];
            Self {
                u8s: u8s.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
                f32s: f32s.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
                reject: HashSet::new(),
                md_locked: 0,
                log: Vec::new(),
            }
        }

        fn empty() -> Self {
            Self {
                u8s: HashMap::new(),
                f32s: HashMap::new(),
                reject: HashSet::new(),
                md_locked: 0,
                log: Vec::new(),
            }
        }

        fn u8(&self, key: &str) -> u8 {
            self.u8s[key]
        }
    }

    impl Smc for FakeSmc {
        fn read_u8(&mut self, key: &str) -> Result<u8, String> {
            self.u8s.get(key).copied().ok_or_else(|| format!("{key}: missing"))
        }

        fn read_f32(&mut self, key: &str) -> Result<f32, String> {
            self.f32s.get(key).copied().ok_or_else(|| format!("{key}: missing"))
        }

        fn write_u8(&mut self, key: &str, value: u8) -> Result<(), String> {
            if self.reject.contains(key) {
                return Err(format!("{key}: rejected"));
            }
            if key.ends_with("Md") && self.md_locked > 0 {
                self.md_locked -= 1;
                return Err(format!("{key}: locked"));
            }
            self.u8s.insert(key.into(), value);
            self.log.push(format!("{key}={value}"));
            Ok(())
        }

        fn write_f32(&mut self, key: &str, value: f32) -> Result<(), String> {
            if self.reject.contains(key) {
                return Err(format!("{key}: rejected"));
            }
            self.f32s.insert(key.into(), value);
            self.log.push(format!("{key}={value}"));
            Ok(())
        }
    }

    #[test]
    fn key_names() {
        assert_eq!(key(0, "Md"), "F0Md");
        assert_eq!(key(1, "Mx"), "F1Mx");
    }

    #[test]
    fn read_reports_fans_and_manual() {
        let mut smc = FakeSmc::two_fans();
        let fans = read(&mut smc).unwrap();
        assert_eq!(fans.len(), 2);
        assert!(fans.iter().all(|f| f.max_rpm == 6800.0));
        assert!(!manual(&fans));
        smc.u8s.insert("F1Md".into(), MANUAL);
        assert!(manual(&read(&mut smc).unwrap()));
    }

    #[test]
    fn read_without_fan_keys_is_unsupported() {
        assert!(read(&mut FakeSmc::empty()).is_err());
        let mut smc = FakeSmc::empty();
        smc.u8s.insert("Ftst".into(), 0);
        smc.u8s.insert("FNum".into(), 0);
        assert_eq!(read(&mut smc).unwrap(), Vec::<Fan>::new());
    }

    #[test]
    fn force_max_sets_target_before_manual_mode_on_every_fan() {
        let mut smc = FakeSmc::two_fans();
        force_max(&mut smc, &mut || {}).unwrap();
        assert_eq!(smc.log, ["Ftst=1", "F0Tg=6800", "F0Md=1", "F1Tg=6800", "F1Md=1"]);
    }

    #[test]
    fn force_max_retries_manual_mode_after_unlock() {
        let mut smc = FakeSmc::two_fans();
        smc.md_locked = 3;
        let mut pauses = 0;
        force_max(&mut smc, &mut || pauses += 1).unwrap();
        assert_eq!(pauses, 3);
        assert_eq!(smc.u8("F0Md"), MANUAL);
        assert_eq!(smc.u8("F1Md"), MANUAL);
    }

    #[test]
    fn force_max_gives_up_and_restores_auto() {
        let mut smc = FakeSmc::two_fans();
        smc.md_locked = usize::MAX;
        let mut pauses = 0;
        assert!(force_max(&mut smc, &mut || pauses += 1).is_err());
        assert_eq!(pauses, TRIES);
        // Still locked, so auto could not be written either; never manual.
        assert_ne!(smc.u8("F0Md"), MANUAL);
        assert_ne!(smc.u8("F1Md"), MANUAL);
        assert_eq!(smc.u8("Ftst"), 0);
    }

    #[test]
    fn failed_target_write_restores_auto() {
        let mut smc = FakeSmc::two_fans();
        smc.reject.insert("F1Tg".into());
        assert!(force_max(&mut smc, &mut || {}).is_err());
        assert_eq!(smc.u8("F0Md"), AUTO);
        assert_ne!(smc.u8("F1Md"), MANUAL);
        assert_eq!(smc.u8("Ftst"), 0);
    }

    #[test]
    fn restore_auto_tries_every_fan_even_if_one_fails() {
        let mut smc = FakeSmc::two_fans();
        force_max(&mut smc, &mut || {}).unwrap();
        smc.reject.insert("F0Md".into());
        assert!(restore_auto(&mut smc).is_err());
        assert_eq!(smc.u8("F1Md"), AUTO);
        assert_eq!(smc.u8("Ftst"), 0);
    }

    #[test]
    fn force_max_without_fans_is_an_error() {
        let mut smc = FakeSmc::empty();
        smc.u8s.insert("Ftst".into(), 0);
        smc.u8s.insert("FNum".into(), 0);
        assert!(force_max(&mut smc, &mut || {}).is_err());
        assert!(smc.log.is_empty());
    }
}
