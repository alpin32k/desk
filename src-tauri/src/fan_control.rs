//! The fans through the SMC: read them, put them all at full speed, give them
//! back to macOS. Shared by desk (reading, without root) and the `desk-fans`
//! helper (writing, as root; src-tauri/fans-helper), which includes this file
//! with `#[path]`, so nothing here may use desk's other modules or `tr!`.
//!
//! M1–M4: writes need `Ftst` = 1 first, and macOS (thermalmonitord)
//! then keeps driving the fans for a few seconds: a write of a fan's mode
//! (`FnMd`, manual = 1) or target speed (`FnTg`) succeeds, but macOS puts its
//! own back (an M3 Pro reads mode 0, target at the minimum). So every round
//! writes manual, then the maximum, and reads them back after a pause, until
//! they stick. The target goes in right after the mode (before it macOS
//! overwrites it), and any failure ends in auto, so a fan never stays manual
//! at a low target. Going back is late too: right after the write of auto
//! the fans still read manual for a moment, so `restore_auto` reads them back
//! until they do not.
//!
//! M5: the mode key is `Fnmd` (lowercase) and there is no `Ftst`; mode and
//! target writes take at once (Stats, exelban/stats#3051, tested on an M5
//! Pro). `Keys` tells the two apart.

/// 2: waits for macOS to let go of the fans (protocol 1 gave up at once) and
/// knows the M5's keys.
pub const PROTOCOL: u32 = 2;
pub const AUTO: u8 = 0;
pub const MANUAL: u8 = 1;
/// Rounds of `max` (and read-backs of `auto`), a pause apart (100 ms in the
/// helper): about 10 s for macOS to let go of the fans or take them back.
pub const TRIES: usize = 100;
/// A fan's maximum below this is not believed (a laptop fan tops out at
/// several thousand rpm; 6800 on an M3 Pro).
pub const MIN_MAX_RPM: f32 = 1000.0;

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

/// How this Mac names its fan keys (Stats probes the same way).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keys {
    /// The mode key is `Fnmd` (M5), not `FnMd` (M1–M4).
    pub lower: bool,
    /// The SMC has `Ftst`, the unlock M1–M4 need before a fan takes manual.
    pub unlock: bool,
}

impl Keys {
    pub fn probe(smc: &mut impl Smc) -> Keys {
        Keys {
            lower: smc.read_u8("F0md").is_ok(),
            unlock: smc.read_u8("Ftst").is_ok(),
        }
    }

    /// A fan's mode key: "F0Md", or "F0md" on an M5.
    pub fn mode(&self, fan: u8) -> String {
        key(fan, if self.lower { "md" } else { "Md" })
    }
}

/// Every fan. Err: this Mac's fans cannot be read (no `FNum`, no mode key);
/// an empty list: it has none.
pub fn read(smc: &mut impl Smc) -> Result<Vec<Fan>, String> {
    let keys = Keys::probe(smc);
    read_with(smc, keys)
}

fn read_with(smc: &mut impl Smc, keys: Keys) -> Result<Vec<Fan>, String> {
    let count = smc.read_u8("FNum")?;
    (0..count)
        .map(|i| {
            let fan = Fan {
                mode: smc.read_u8(&keys.mode(i))?,
                rpm: smc.read_f32(&key(i, "Ac"))?,
                max_rpm: smc.read_f32(&key(i, "Mx"))?,
            };
            // A maximum no fan has would make "max" a low manual speed.
            if !(fan.max_rpm.is_finite() && fan.max_rpm >= MIN_MAX_RPM) {
                return Err(format!("fan {i}: implausible maximum {}", fan.max_rpm));
            }
            Ok(fan)
        })
        .collect()
}

/// Some fan is in manual mode (desk's or another program's doing).
pub fn manual(fans: &[Fan]) -> bool {
    fans.iter().any(|f| f.mode == MANUAL)
}

/// Every fan at full speed. On any failure the fans go back to auto first.
pub fn force_max(smc: &mut impl Smc, pause: &mut dyn FnMut()) -> Result<(), String> {
    let keys = Keys::probe(smc);
    let fans = read_with(smc, keys)?;
    if fans.is_empty() {
        return Err("no fans".into());
    }
    let result = apply_max(smc, keys, &fans, pause);
    if result.is_err() {
        let _ = restore_with(smc, keys, pause);
    }
    result
}

fn apply_max(smc: &mut impl Smc, keys: Keys, fans: &[Fan], pause: &mut dyn FnMut()) -> Result<(), String> {
    // M1–M4; an M5 has no Ftst and takes the writes as they are.
    if keys.unlock {
        smc.write_u8("Ftst", 1)?;
    }
    let mut last = String::new();
    for _ in 0..TRIES {
        let locked = set_max(smc, keys, fans)?;
        pause();
        last = match locked {
            Some(e) => e,
            None => match not_applied(smc, keys, fans)? {
                None => return Ok(()),
                Some(why) => why,
            },
        };
    }
    Err(last)
}

/// Every fan manual, then at its maximum. Some: a mode write was refused
/// (still locked after the unlock), so the round is cut short; a refused
/// target write ends the try.
fn set_max(smc: &mut impl Smc, keys: Keys, fans: &[Fan]) -> Result<Option<String>, String> {
    for (i, fan) in (0u8..).zip(fans) {
        if let Err(e) = smc.write_u8(&keys.mode(i), MANUAL) {
            return Ok(Some(e));
        }
        smc.write_f32(&key(i, "Tg"), fan.max_rpm)?;
    }
    Ok(None)
}

/// None: every fan reads manual at its maximum; Some: the first that does not.
fn not_applied(smc: &mut impl Smc, keys: Keys, fans: &[Fan]) -> Result<Option<String>, String> {
    for (i, fan) in (0u8..).zip(fans) {
        let mode = smc.read_u8(&keys.mode(i))?;
        let target = smc.read_f32(&key(i, "Tg"))?;
        if mode != MANUAL || (target - fan.max_rpm).abs() >= 1.0 {
            return Ok(Some(format!("fan {i}: not applied (mode {mode}, target {target})")));
        }
    }
    Ok(None)
}

/// Every fan back in auto, then the SMC locked again. Tries every fan even
/// when one fails, and returns the first error. The mode written shows only
/// after a moment (an M3 Pro still reads manual right after), so the fans are
/// read back a pause apart, at most TRIES times.
pub fn restore_auto(smc: &mut impl Smc, pause: &mut dyn FnMut()) -> Result<(), String> {
    let keys = Keys::probe(smc);
    restore_with(smc, keys, pause)
}

fn restore_with(smc: &mut impl Smc, keys: Keys, pause: &mut dyn FnMut()) -> Result<(), String> {
    let count = smc.read_u8("FNum")?;
    let mut first: Result<(), String> = Ok(());
    for i in 0..count {
        let r = smc.write_u8(&keys.mode(i), AUTO);
        if first.is_ok() {
            first = r;
        }
    }
    if keys.unlock {
        let r = smc.write_u8("Ftst", 0);
        if first.is_ok() {
            first = r;
        }
    }
    first?;
    let mut last = String::new();
    for _ in 0..TRIES {
        match still_manual(smc, keys, count)? {
            None => return Ok(()),
            Some(why) => last = why,
        }
        pause();
    }
    Err(last)
}

/// The first fan still reading manual, if any.
fn still_manual(smc: &mut impl Smc, keys: Keys, count: u8) -> Result<Option<String>, String> {
    for i in 0..count {
        if smc.read_u8(&keys.mode(i))? == MANUAL {
            return Ok(Some(format!("fan {i}: still manual")));
        }
    }
    Ok(None)
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
        /// macOS still drives the fans for the next N writes to any `F?Md`:
        /// they and the target writes succeed but do not stick (an M3 Pro
        /// right after `Ftst` = 1).
        held: usize,
        /// The next N reads of any `F?Md` still say manual (an M3 Pro right
        /// after the write of auto).
        stale_reads: usize,
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
                held: 0,
                stale_reads: 0,
                log: Vec::new(),
            }
        }

        /// An M5: the mode key is `Fnmd`, there is no `Ftst`, and writes take
        /// at once.
        fn m5() -> Self {
            let mut smc = Self::two_fans();
            for k in ["Ftst", "F0Md", "F1Md"] {
                smc.u8s.remove(k);
            }
            smc.u8s.insert("F0md".into(), AUTO);
            smc.u8s.insert("F1md".into(), AUTO);
            smc
        }

        fn empty() -> Self {
            Self {
                u8s: HashMap::new(),
                f32s: HashMap::new(),
                reject: HashSet::new(),
                md_locked: 0,
                held: 0,
                stale_reads: 0,
                log: Vec::new(),
            }
        }

        fn u8(&self, key: &str) -> u8 {
            self.u8s[key]
        }
    }

    impl Smc for FakeSmc {
        fn read_u8(&mut self, key: &str) -> Result<u8, String> {
            if key.ends_with("Md") && self.stale_reads > 0 {
                self.stale_reads -= 1;
                return Ok(MANUAL);
            }
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
            if key.ends_with("Md") && self.held > 0 {
                self.held -= 1;
                return Ok(());
            }
            self.u8s.insert(key.into(), value);
            self.log.push(format!("{key}={value}"));
            Ok(())
        }

        fn write_f32(&mut self, key: &str, value: f32) -> Result<(), String> {
            if self.reject.contains(key) {
                return Err(format!("{key}: rejected"));
            }
            if key.ends_with("Tg") && self.held > 0 {
                return Ok(());
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
    fn implausible_max_is_unsupported() {
        for bad in [0.0, f32::NAN, 500.0] {
            let mut smc = FakeSmc::two_fans();
            smc.f32s.insert("F0Mx".into(), bad);
            assert!(read(&mut smc).is_err(), "{bad}");
            assert!(force_max(&mut smc, &mut || {}).is_err());
            assert!(smc.log.is_empty(), "{bad}: {:?}", smc.log);
        }
    }

    #[test]
    fn force_max_sets_every_fan_manual_then_at_its_maximum() {
        let mut smc = FakeSmc::two_fans();
        force_max(&mut smc, &mut || {}).unwrap();
        assert_eq!(smc.log, ["Ftst=1", "F0Md=1", "F0Tg=6800", "F1Md=1", "F1Tg=6800"]);
    }

    #[test]
    fn force_max_retries_manual_mode_after_unlock() {
        let mut smc = FakeSmc::two_fans();
        smc.md_locked = 3;
        let mut pauses = 0;
        force_max(&mut smc, &mut || pauses += 1).unwrap();
        // Three while locked, one to let the last round settle.
        assert_eq!(pauses, 4);
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
        assert!(restore_auto(&mut smc, &mut || {}).is_err());
        assert_eq!(smc.u8("F1Md"), AUTO);
        assert_eq!(smc.u8("Ftst"), 0);
    }

    #[test]
    fn force_max_waits_until_macos_lets_go_of_the_fans() {
        let mut smc = FakeSmc::two_fans();
        smc.held = 5;
        force_max(&mut smc, &mut || {}).unwrap();
        assert_eq!(smc.u8("F0Md"), MANUAL);
        assert_eq!(smc.u8("F1Md"), MANUAL);
        assert_eq!(smc.f32s["F0Tg"], 6800.0);
        assert_eq!(smc.f32s["F1Tg"], 6800.0);
    }

    #[test]
    fn force_max_gives_up_if_macos_never_lets_go() {
        let mut smc = FakeSmc::two_fans();
        smc.held = usize::MAX;
        let mut pauses = 0;
        assert!(force_max(&mut smc, &mut || pauses += 1).is_err());
        assert_eq!(pauses, TRIES);
        assert_ne!(smc.u8("F0Md"), MANUAL);
        assert_ne!(smc.u8("F1Md"), MANUAL);
        assert_eq!(smc.u8("Ftst"), 0);
    }

    #[test]
    fn restore_auto_waits_until_the_fans_read_auto() {
        let mut smc = FakeSmc::two_fans();
        force_max(&mut smc, &mut || {}).unwrap();
        smc.stale_reads = 3;
        let mut pauses = 0;
        restore_auto(&mut smc, &mut || pauses += 1).unwrap();
        assert_eq!(pauses, 3);
        assert_eq!(smc.u8("F0Md"), AUTO);
        assert_eq!(smc.u8("Ftst"), 0);
    }

    #[test]
    fn restore_auto_gives_up_on_a_fan_that_stays_manual() {
        let mut smc = FakeSmc::two_fans();
        force_max(&mut smc, &mut || {}).unwrap();
        smc.stale_reads = usize::MAX;
        let mut pauses = 0;
        assert!(restore_auto(&mut smc, &mut || pauses += 1).is_err());
        assert_eq!(pauses, TRIES);
    }

    #[test]
    fn keys_tell_an_m5_from_an_m1_to_m4() {
        let old = Keys::probe(&mut FakeSmc::two_fans());
        assert_eq!((old.mode(1), old.unlock), ("F1Md".to_string(), true));
        let m5 = Keys::probe(&mut FakeSmc::m5());
        assert_eq!((m5.mode(1), m5.unlock), ("F1md".to_string(), false));
    }

    #[test]
    fn read_on_an_m5_uses_the_lowercase_mode_key() {
        let mut smc = FakeSmc::m5();
        let fans = read(&mut smc).unwrap();
        assert_eq!(fans.len(), 2);
        assert!(!manual(&fans));
        smc.u8s.insert("F1md".into(), MANUAL);
        assert!(manual(&read(&mut smc).unwrap()));
    }

    #[test]
    fn an_m5_goes_to_max_and_back_without_ftst() {
        let mut smc = FakeSmc::m5();
        force_max(&mut smc, &mut || {}).unwrap();
        assert_eq!(smc.log, ["F0md=1", "F0Tg=6800", "F1md=1", "F1Tg=6800"]);
        smc.log.clear();
        restore_auto(&mut smc, &mut || {}).unwrap();
        assert_eq!(smc.log, ["F0md=0", "F1md=0"]);
        assert!(!smc.u8s.contains_key("Ftst"));
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
