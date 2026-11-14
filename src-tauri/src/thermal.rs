//! Temperature guard: every 5 s desk reads the CPU temperature, shows it in
//! the top bar and, while the computer stays hotter than the limit set in
//! Options (75 °C by default), slows the work running in its sessions (tests,
//! builds, whatever an agent starts) until it cools down. It keeps running,
//! only slower, and nothing outside desk's sessions is touched. What you type
//! into (the shell, the agent, an editor) keeps full speed: see `split`.
//!
//! Reading
//! - macOS: the CPU die sensors ("PMU tdie*" on Apple Silicon) through
//!   IOHIDEventSystemClient, as Stats or macmon read them; no admin rights.
//!   The system's own thermal state counts too: serious or worse is over the
//!   limit whatever the sensors say (Intel Macs have no such sensors).
//! - Linux: the CPU hwmon drivers (coretemp, k10temp...), else the hottest
//!   thermal zone.
//! - Windows: the ACPI thermal zones performance counter. Many machines have
//!   none; then the bar shows no temperature and nothing is slowed.
//!
//! Slowing
//! - macOS: `PRIO_DARWIN_BG`, what `taskpolicy -b` sets: low CPU and disk
//!   priority on slower cores at a lower clock; new child processes inherit it.
//! - Windows: EcoQoS, Task Manager's efficiency mode, on each process.
//! - Linux: the processes move into the systemd user scope
//!   `desk-sessions.scope`, capped at half the CPU cores. Without systemd's
//!   user manager nothing is slowed and the bar says why.
//!
//! The sessions get full speed back 7 °C below the limit (after at least a
//! minute, so it does not flap), when slowing is turned off, and when desk
//! quits. After a crash the next start puts them back.
//!
//! Fans
//! - Only when the user picked a mode with fans in Options (`Mode`): every
//!   fan at full speed from its own limit, back to auto by the same rules as
//!   slowing (fans.rs; macOS on Apple Silicon only). A second `Policy` decides
//!   for the fans, independent of the slowing one.

use serde::Serialize;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use crate::fans::{FanStatus, Fans};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const DEFAULT_LIMIT: u32 = 75;
/// Lowest and highest limit Options accepts.
pub const MIN_LIMIT: u32 = 50;
pub const MAX_LIMIT: u32 = 100;
/// Where the fans start in `Mode::FansFirst`, unless Options say otherwise.
pub const DEFAULT_FAN_LIMIT: u32 = 70;
/// Back to full speed this far below the limit.
pub const COOL_MARGIN: f32 = 7.0;
const TICK: Duration = Duration::from_secs(5);
/// Readings averaged before deciding (15 s): one spike slows nothing.
const WINDOW: usize = 3;
/// Slowed at least this long, so the sessions do not flap at the limit.
const MIN_SLOW: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Hottest CPU sensor in °C; None when desk cannot read one here.
    pub celsius: Option<i32>,
    /// Slowing limit; 0: this mode never slows.
    pub limit: u32,
    /// Fan limit; 0: this mode never uses the fans.
    pub fan_limit: u32,
    /// Too hot: desk is slowing its sessions.
    pub slowed: bool,
    /// Processes slowed (what the sessions' shells, agents and editors started).
    pub processes: usize,
    /// Why desk cannot slow anything here, if it cannot.
    pub cannot_slow: Option<String>,
    /// The fans, when the mode uses them.
    pub fans: Option<FanStatus>,
}

/// What desk does when hot (Options, layout `thermalMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Off,
    Slow,
    /// Fans from their own, lower limit; slowing from the limit.
    FansFirst,
    Together,
    Fans,
}

impl Mode {
    /// The layout's name; anything unknown is `Slow`, today's behaviour.
    pub fn parse(s: &str) -> Mode {
        match s {
            "off" => Mode::Off,
            "fansFirst" => Mode::FansFirst,
            "together" => Mode::Together,
            "fans" => Mode::Fans,
            _ => Mode::Slow,
        }
    }

    /// What the mode does besides the fans (the permission was removed).
    pub fn without_fans(self) -> Mode {
        match self {
            Mode::FansFirst | Mode::Together => Mode::Slow,
            Mode::Fans => Mode::Off,
            m => m,
        }
    }
}

/// (slowing limit, fan limit) for a mode; 0: not used. Both within
/// MIN_LIMIT..=MAX_LIMIT, and the fans never wait longer than the slowing.
pub fn limits(mode: Mode, limit: u32, fan_limit: u32) -> (u32, u32) {
    let limit = limit.clamp(MIN_LIMIT, MAX_LIMIT);
    let fan = fan_limit.clamp(MIN_LIMIT, MAX_LIMIT).min(limit);
    match mode {
        Mode::Off => (0, 0),
        Mode::Slow => (limit, 0),
        Mode::FansFirst => (limit, fan),
        Mode::Together => (limit, limit),
        Mode::Fans => (0, limit),
    }
}

/// When to slow: the average of the last readings at or over the limit (or
/// the system saying it is too hot), and when to stop.
#[derive(Debug, Default)]
pub struct Policy {
    readings: VecDeque<f32>,
    since: Option<Instant>,
}

impl Policy {
    /// Takes a reading (None: no sensor) and the system's own "too hot";
    /// says whether the sessions should be slowed now.
    pub fn step(&mut self, celsius: Option<f32>, pressure: bool, limit: u32, now: Instant) -> bool {
        if let Some(c) = celsius {
            if self.readings.len() == WINDOW {
                self.readings.pop_front();
            }
            self.readings.push_back(c);
        }
        if limit == 0 {
            self.since = None;
            return false;
        }
        let avg = (self.readings.len() == WINDOW).then(|| self.readings.iter().sum::<f32>() / WINDOW as f32);
        let limit = limit as f32;
        match self.since {
            None if pressure || avg.is_some_and(|a| a >= limit) => self.since = Some(now),
            Some(start) => {
                let cool = !pressure && avg.is_none_or(|a| a <= limit - COOL_MARGIN);
                if cool && now.duration_since(start) >= MIN_SLOW {
                    self.since = None;
                }
            }
            None => {}
        }
        self.since.is_some()
    }
}

/// One running process: what desk needs to know to decide about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Proc {
    pub pid: u32,
    pub parent: u32,
    /// The program's file name: "zsh", "claude", "node".
    pub name: String,
    /// Its terminal ("ttys004", "pts/3"); None without one or where desk
    /// cannot tell (Windows).
    pub tty: Option<String>,
}

/// The given processes still running and everything they started.
pub fn descendants(roots: &[u32], table: &[Proc]) -> Vec<u32> {
    let alive: HashSet<u32> = table.iter().map(|p| p.pid).collect();
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for p in table {
        if p.pid != p.parent {
            children.entry(p.parent).or_default().push(p.pid);
        }
    }
    let mut seen = BTreeSet::new();
    let mut stack: Vec<u32> = roots.iter().copied().filter(|p| alive.contains(p)).collect();
    while let Some(pid) = stack.pop() {
        // Never pid 0/1 (the kernel, launchd/init) nor desk itself.
        if pid <= 1 || pid == std::process::id() || !seen.insert(pid) {
            continue;
        }
        if let Some(c) = children.get(&pid) {
            stack.extend(c);
        }
    }
    seen.into_iter().collect()
}

/// The sessions' processes: the ones to slow and the ones to leave alone.
#[derive(Debug, Default, PartialEq)]
pub struct Split {
    pub slow: Vec<u32>,
    pub spare: Vec<u32>,
}

/// Programs that only start other programs in a terminal.
const SHELLS: &[&str] = &[
    "sh", "bash", "zsh", "fish", "dash", "ksh", "tcsh", "csh", "nu", "elvish", "xonsh", "pwsh", "powershell", "cmd",
];

/// What to slow among the sessions' processes (`roots` and everything they
/// started), so that typing never stutters: the work, not what you type into.
/// - A session's shell and the shells it starts are never slowed.
/// - The program a shell runs (an agent, an editor) is left alone while its
///   terminal takes keys one at a time (`typing(tty)`): you may be typing
///   into it. With the terminal in line mode it is a job (`pnpm test`).
/// - Everything such a program starts is slowed: tests, builds, tools.
pub fn split(roots: &[u32], table: &[Proc], typing: impl Fn(Option<&str>) -> bool) -> Split {
    let by_pid: HashMap<u32, &Proc> = table.iter().map(|p| (p.pid, p)).collect();
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for p in table {
        if p.pid != p.parent {
            children.entry(p.parent).or_default().push(p.pid);
        }
    }
    let mut typed: HashMap<Option<&str>, bool> = HashMap::new();
    let mut spare = BTreeSet::new();
    // Shells first, then the programs they run; `fronts` are those candidates.
    let mut fronts: Vec<u32> = roots.to_vec();
    while let Some(pid) = fronts.pop() {
        let Some(p) = by_pid.get(&pid) else { continue };
        if pid <= 1 || pid == std::process::id() || spare.contains(&pid) {
            continue;
        }
        if SHELLS.contains(&p.name.as_str()) {
            spare.insert(pid);
            fronts.extend(children.get(&pid).into_iter().flatten());
        } else if *typed.entry(p.tty.as_deref()).or_insert_with(|| typing(p.tty.as_deref())) {
            spare.insert(pid);
        }
    }
    let slow = descendants(roots, table).into_iter().filter(|p| !spare.contains(p)).collect();
    Split {
        slow,
        spare: spare.into_iter().collect(),
    }
}

/// `ps -A -o pid=,ppid=,tty=,comm=` output.
pub fn parse_ps(out: &str) -> Vec<Proc> {
    out.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let pid = it.next()?.parse().ok()?;
            let parent = it.next()?.parse().ok()?;
            let tty = it.next()?;
            // The rest is the program, a path with spaces at times.
            let comm = it.collect::<Vec<_>>().join(" ");
            Some(Proc {
                pid,
                parent,
                name: program_name(&comm),
                tty: (!tty.starts_with('?')).then(|| tty.to_string()),
            })
        })
        .collect()
}

/// "/bin/zsh", "-zsh" (a login shell) and "pwsh.exe" are zsh, zsh and pwsh.
pub fn program_name(comm: &str) -> String {
    let file = comm.rsplit(['/', '\\']).next().unwrap_or(comm);
    let file = file.strip_prefix('-').unwrap_or(file);
    let lower = file.to_lowercase();
    lower.strip_suffix(".exe").unwrap_or(&lower).to_string()
}

/// The hottest believable reading (sensors that are off report nonsense).
pub fn hottest(celsius: impl IntoIterator<Item = f32>) -> Option<f32> {
    celsius
        .into_iter()
        .filter(|c| (1.0..=130.0).contains(c))
        .reduce(f32::max)
}

/// macOS: Apple Silicon names its CPU die sensors "PMU tdie1", "PMU tdie2"...
pub fn is_cpu_sensor(name: &str) -> bool {
    name.contains("tdie")
}

/// Linux hwmon drivers that report the CPU itself.
const CPU_DRIVERS: &[&str] = &["coretemp", "k10temp", "zenpower", "cpu_thermal", "cpu-thermal", "soc_thermal"];

/// Linux: the CPU drivers' sensors (name, millidegrees) if there are any,
/// else the hottest thermal zone (type, millidegrees).
pub fn pick_linux(hwmon: &[(String, Vec<i64>)], zones: &[(String, i64)]) -> Option<f32> {
    let c = |m: i64| m as f32 / 1000.0;
    let cpu = hottest(
        hwmon
            .iter()
            .filter(|(name, _)| CPU_DRIVERS.contains(&name.as_str()))
            .flat_map(|(_, temps)| temps.iter().map(|&m| c(m))),
    );
    cpu.or_else(|| hottest(zones.iter().map(|&(_, m)| c(m))))
}

/// Linux: the CPU quota for slowed sessions, half the cores, in µs per second.
pub fn quota_usec(cpus: usize) -> u64 {
    (cpus / 2).max(1) as u64 * 1_000_000
}

pub const SCOPE: &str = "desk-sessions.scope";
const SYSTEMD: [&str; 4] = [
    "org.freedesktop.systemd1",
    "/org/freedesktop/systemd1",
    "org.freedesktop.systemd1.Manager",
    "",
];

/// `busctl --user call` arguments for systemd's Manager `method` with `rest`.
fn busctl_call(method: &str, rest: Vec<String>) -> Vec<String> {
    let mut v: Vec<String> = ["--user", "call"].map(String::from).to_vec();
    v.extend(SYSTEMD[..3].iter().map(|s| s.to_string()));
    v.push(method.into());
    v.extend(rest);
    v
}

/// A new scope holding `pids`, capped at `quota` µs of CPU per second.
pub fn start_args(pids: &[u32], quota: u64) -> Vec<String> {
    let mut rest: Vec<String> = ["ssa(sv)a(sa(sv))", SCOPE, "fail", "2", "PIDs", "au"].map(String::from).to_vec();
    rest.push(pids.len().to_string());
    rest.extend(pids.iter().map(u32::to_string));
    rest.extend(["CPUQuotaPerSecUSec".into(), "t".into(), quota.to_string(), "0".into()]);
    busctl_call("StartTransientUnit", rest)
}

/// Move `pids` into the existing scope.
pub fn attach_args(pids: &[u32]) -> Vec<String> {
    let mut rest: Vec<String> = ["ssau", SCOPE, SYSTEMD[3]].map(String::from).to_vec();
    rest.push(pids.len().to_string());
    rest.extend(pids.iter().map(u32::to_string));
    busctl_call("AttachProcessesToUnit", rest)
}

/// Set the scope's cap; u64::MAX lifts it.
pub fn quota_args(quota: u64) -> Vec<String> {
    let rest = ["sba(sv)", SCOPE, "true", "1", "CPUQuotaPerSecUSec", "t"].map(String::from);
    let mut rest = rest.to_vec();
    rest.push(quota.to_string());
    busctl_call("SetUnitProperties", rest)
}

/// Linux: `/proc/<pid>/cgroup` of a process already in desk's scope.
pub fn in_scope(cgroup: &str) -> bool {
    cgroup.lines().any(|l| l.ends_with(&format!("/{SCOPE}")))
}

struct Inner {
    status: Status,
    policy: Policy,
    fan_policy: Policy,
    fans: Fans,
    os: os::Slower,
    /// The first reading after start: put back what a crashed desk left slowed.
    first: bool,
    /// desk is quitting: nothing more is slowed.
    stopped: bool,
}

pub struct Guard {
    /// From Options: the mode, the slowing limit and the fan limit.
    config: Mutex<(Mode, u32, u32)>,
    inner: Mutex<Inner>,
}

impl Guard {
    pub fn new(mode: Mode, limit: u32, fan_limit: u32, support_dir: &Path) -> Self {
        let (slow, fan) = limits(mode, limit, fan_limit);
        Self {
            config: Mutex::new((mode, limit, fan_limit)),
            inner: Mutex::new(Inner {
                status: Status {
                    celsius: None,
                    limit: slow,
                    fan_limit: fan,
                    slowed: false,
                    processes: 0,
                    cannot_slow: None,
                    fans: None,
                },
                policy: Policy::default(),
                fan_policy: Policy::default(),
                fans: Fans::new(support_dir),
                // A unit struct on macOS and Windows, with state on Linux.
                #[allow(clippy::default_constructed_unit_structs)]
                os: os::Slower::default(),
                first: true,
                stopped: false,
            }),
        }
    }

    pub fn configure(&self, mode: Mode, limit: u32, fan_limit: u32) {
        *self.config.lock().unwrap() = (mode, limit, fan_limit);
    }

    /// Before removing the permission: the fans back to auto now, if desk set
    /// them, and no forcing until `resume_fans`.
    pub fn pause_fans(&self) {
        self.inner.lock().unwrap().fans.pause();
    }

    pub fn resume_fans(&self) {
        self.inner.lock().unwrap().fans.resume();
    }

    /// The permission is gone: the mode keeps only what it did besides the
    /// fans (Options saves the same).
    pub fn drop_fan_mode(&self) {
        let mut config = self.config.lock().unwrap();
        config.0 = config.0.without_fans();
    }

    pub fn status(&self) -> Status {
        self.inner.lock().unwrap().status.clone()
    }

    /// One reading: decide, then slow the sessions' processes or give them
    /// their speed back. `roots`: the processes the sessions run. Returns the
    /// new status when it changed.
    pub fn tick(&self, celsius: Option<f32>, pressure: bool, roots: impl FnOnce() -> Vec<u32>) -> Option<Status> {
        let mut g = self.inner.lock().unwrap();
        if g.stopped {
            return None;
        }
        let (mode, limit, fan_limit) = *self.config.lock().unwrap();
        let (limit, fan_limit) = limits(mode, limit, fan_limit);
        let now = Instant::now();
        let was = g.status.slowed;
        let slow = g.policy.step(celsius, pressure, limit, now);
        let want_fans = g.fan_policy.step(celsius, pressure, fan_limit, now);
        if g.first {
            g.fans.recover();
        }
        // Also when the mode just dropped the fans: step releases them.
        let fans = (fan_limit > 0 || g.fans.ours()).then(|| g.fans.step(want_fans, now));
        let mut next = Status {
            celsius: celsius.map(|c| c.round() as i32),
            limit,
            fan_limit,
            slowed: slow,
            processes: 0,
            cannot_slow: g.status.cannot_slow.clone(),
            fans: fans.filter(|_| fan_limit > 0),
        };
        if slow {
            let parts = split(&roots(), &os::processes(), os::typing);
            g.os.spare(&parts.spare);
            match g.os.slow(&parts.slow) {
                Ok(n) => {
                    next.processes = n;
                    next.cannot_slow = None;
                }
                Err(e) => next.cannot_slow = Some(e),
            }
        } else if was || g.first {
            let pids = descendants(&roots(), &os::processes());
            g.os.release(&pids);
        }
        g.first = false;
        if next == g.status {
            return None;
        }
        g.status = next.clone();
        Some(next)
    }

    /// desk quits: the sessions (tmux keeps them running) get full speed back
    /// and the fans go back to auto.
    pub fn shutdown(&self, roots: &[u32]) {
        let mut g = self.inner.lock().unwrap();
        g.stopped = true;
        g.fans.release();
        if g.status.slowed {
            let pids = descendants(roots, &os::processes());
            g.os.release(&pids);
        }
    }
}

/// Reads every 5 s on its own thread; `tick` gets the reading and the
/// system's own "too hot".
pub fn spawn(mut tick: impl FnMut(Option<f32>, bool) + Send + 'static) {
    std::thread::spawn(move || {
        let mut sensor = os::Sensor::new();
        loop {
            tick(sensor.read(), os::pressure());
            std::thread::sleep(TICK);
        }
    });
}

/// Whether the terminal takes keys one at a time (raw mode: a shell prompt,
/// an editor, an agent) rather than whole lines (a job printing output).
/// A terminal desk cannot ask counts as typed into: better a job at full
/// speed than stuttering keys.
#[cfg(unix)]
fn tty_takes_keys(tty: Option<&str>) -> bool {
    let Some(tty) = tty else { return false };
    let Ok(path) = std::ffi::CString::new(format!("/dev/{tty}")) else {
        return true;
    };
    // SAFETY: opens and closes a descriptor of its own; O_NOCTTY keeps the
    // terminal from becoming desk's, O_NONBLOCK from waiting on it.
    unsafe {
        let fd = libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_NOCTTY | libc::O_NONBLOCK);
        if fd < 0 {
            return true;
        }
        let mut t: libc::termios = std::mem::zeroed();
        let ok = libc::tcgetattr(fd, &mut t) == 0;
        libc::close(fd);
        !ok || t.c_lflag & libc::ICANON == 0
    }
}

#[cfg(unix)]
fn ps_processes() -> Vec<Proc> {
    std::process::Command::new("ps")
        .args(["-A", "-o", "pid=,ppid=,tty=,comm="])
        .output()
        .map(|o| parse_ps(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
mod os {
    use super::{hottest, is_cpu_sensor};
    use std::ffi::{CStr, c_char, c_void};
    use std::ptr::null;

    type CFRef = *const c_void;
    const NUMBER_SINT32: isize = 3;
    const UTF8: u32 = 0x0800_0100;
    const EVENT_TEMPERATURE: i64 = 15;

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOHIDEventSystemClientCreate(allocator: CFRef) -> CFRef;
        fn IOHIDEventSystemClientSetMatching(client: CFRef, matching: CFRef) -> i32;
        fn IOHIDEventSystemClientCopyServices(client: CFRef) -> CFRef;
        fn IOHIDServiceClientCopyProperty(service: CFRef, key: CFRef) -> CFRef;
        fn IOHIDServiceClientCopyEvent(service: CFRef, kind: i64, options: i32, timestamp: i64) -> CFRef;
        fn IOHIDEventGetFloatValue(event: CFRef, field: i32) -> f64;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFTypeDictionaryKeyCallBacks: u8;
        static kCFTypeDictionaryValueCallBacks: u8;
        fn CFNumberCreate(allocator: CFRef, kind: isize, value: *const c_void) -> CFRef;
        fn CFDictionaryCreate(
            allocator: CFRef,
            keys: *const CFRef,
            values: *const CFRef,
            count: isize,
            key_callbacks: *const u8,
            value_callbacks: *const u8,
        ) -> CFRef;
        fn CFStringCreateWithCString(allocator: CFRef, s: *const c_char, encoding: u32) -> CFRef;
        fn CFStringGetCString(s: CFRef, buf: *mut c_char, size: isize, encoding: u32) -> u8;
        fn CFArrayGetCount(array: CFRef) -> isize;
        fn CFArrayGetValueAtIndex(array: CFRef, i: isize) -> CFRef;
        fn CFRetain(x: CFRef) -> CFRef;
        fn CFRelease(x: CFRef);
    }

    unsafe fn cf_str(s: &CStr) -> CFRef {
        unsafe { CFStringCreateWithCString(null(), s.as_ptr(), UTF8) }
    }

    unsafe fn cf_int(n: i32) -> CFRef {
        unsafe { CFNumberCreate(null(), NUMBER_SINT32, (&raw const n).cast()) }
    }

    unsafe fn rust_str(s: CFRef) -> String {
        let mut buf = [0 as c_char; 128];
        if unsafe { CFStringGetCString(s, buf.as_mut_ptr(), buf.len() as isize, UTF8) } == 0 {
            return String::new();
        }
        unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned()
    }

    /// The CPU die sensors, found once and read every tick.
    pub struct Sensor {
        client: CFRef,
        dies: Vec<CFRef>,
    }

    impl Sensor {
        pub fn new() -> Self {
            let mut dies = Vec::new();
            // SAFETY: CoreFoundation create/copy results are released below;
            // the sensors kept are retained and released in Drop.
            unsafe {
                let client = IOHIDEventSystemClientCreate(null());
                if client.is_null() {
                    return Self { client, dies };
                }
                // Apple's vendor page (0xff00), usage 5: temperature sensors.
                let keys = [cf_str(c"PrimaryUsagePage"), cf_str(c"PrimaryUsage")];
                let values = [cf_int(0xff00), cf_int(5)];
                let matching = CFDictionaryCreate(
                    null(),
                    keys.as_ptr(),
                    values.as_ptr(),
                    2,
                    &raw const kCFTypeDictionaryKeyCallBacks,
                    &raw const kCFTypeDictionaryValueCallBacks,
                );
                IOHIDEventSystemClientSetMatching(client, matching);
                for x in keys.into_iter().chain(values).chain([matching]) {
                    CFRelease(x);
                }
                let services = IOHIDEventSystemClientCopyServices(client);
                if !services.is_null() {
                    let product = cf_str(c"Product");
                    for i in 0..CFArrayGetCount(services) {
                        let service = CFArrayGetValueAtIndex(services, i);
                        let name = IOHIDServiceClientCopyProperty(service, product);
                        if name.is_null() {
                            continue;
                        }
                        if is_cpu_sensor(&rust_str(name)) {
                            dies.push(CFRetain(service));
                        }
                        CFRelease(name);
                    }
                    CFRelease(product);
                    CFRelease(services);
                }
                Self { client, dies }
            }
        }

        pub fn read(&mut self) -> Option<f32> {
            hottest(self.dies.iter().filter_map(|&service| {
                // SAFETY: `service` is retained by self; the event is released.
                unsafe {
                    let event = IOHIDServiceClientCopyEvent(service, EVENT_TEMPERATURE, 0, 0);
                    if event.is_null() {
                        return None;
                    }
                    let c = IOHIDEventGetFloatValue(event, (EVENT_TEMPERATURE as i32) << 16);
                    CFRelease(event);
                    Some(c as f32)
                }
            }))
        }
    }

    impl Drop for Sensor {
        fn drop(&mut self) {
            // SAFETY: each was retained (or created) by this Sensor.
            unsafe {
                for &s in &self.dies {
                    CFRelease(s);
                }
                if !self.client.is_null() {
                    CFRelease(self.client);
                }
            }
        }
    }

    /// The system's thermal state is serious or critical.
    pub fn pressure() -> bool {
        use objc2_foundation::{NSProcessInfo, NSProcessInfoThermalState};
        NSProcessInfo::processInfo().thermalState().0 >= NSProcessInfoThermalState::Serious.0
    }

    pub fn processes() -> Vec<super::Proc> {
        super::ps_processes()
    }

    pub fn typing(tty: Option<&str>) -> bool {
        super::tty_takes_keys(tty)
    }

    #[derive(Default)]
    pub struct Slower;

    impl Slower {
        pub fn slow(&mut self, pids: &[u32]) -> Result<usize, String> {
            Ok(pids.iter().filter(|&&p| set_background(p, true)).count())
        }

        pub fn release(&mut self, pids: &[u32]) {
            for &p in pids {
                set_background(p, false);
            }
        }

        /// Full speed for what is never slowed: one desk slowed before this
        /// rule, or a program that went from a job to taking keys.
        pub fn spare(&mut self, pids: &[u32]) {
            self.release(pids);
        }
    }

    fn set_background(pid: u32, on: bool) -> bool {
        let prio = if on { libc::PRIO_DARWIN_BG } else { 0 };
        // SAFETY: plain syscall on a pid; a gone or foreign process just fails.
        unsafe { libc::setpriority(libc::PRIO_DARWIN_PROCESS, pid, prio) == 0 }
    }
}

#[cfg(target_os = "linux")]
mod os {
    use super::{attach_args, in_scope, pick_linux, quota_args, quota_usec, start_args};
    use std::fs;
    use std::process::Command;

    pub struct Sensor;

    impl Sensor {
        pub fn new() -> Self {
            Self
        }

        pub fn read(&mut self) -> Option<f32> {
            let read = |p: std::path::PathBuf| fs::read_to_string(p).ok().map(|s| s.trim().to_string());
            let hwmon: Vec<(String, Vec<i64>)> = fs::read_dir("/sys/class/hwmon")
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| {
                    let dir = e.path();
                    let name = read(dir.join("name"))?;
                    let temps = fs::read_dir(&dir)
                        .into_iter()
                        .flatten()
                        .flatten()
                        .filter(|f| {
                            let n = f.file_name().to_string_lossy().into_owned();
                            n.starts_with("temp") && n.ends_with("_input")
                        })
                        .filter_map(|f| read(f.path())?.parse().ok())
                        .collect();
                    Some((name, temps))
                })
                .collect();
            let zones: Vec<(String, i64)> = fs::read_dir("/sys/class/thermal")
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("thermal_zone"))
                .filter_map(|e| {
                    let kind = read(e.path().join("type")).unwrap_or_default();
                    Some((kind, read(e.path().join("temp"))?.parse().ok()?))
                })
                .collect();
            pick_linux(&hwmon, &zones)
        }
    }

    pub fn pressure() -> bool {
        false
    }

    pub fn processes() -> Vec<super::Proc> {
        super::ps_processes()
    }

    pub fn typing(tty: Option<&str>) -> bool {
        super::tty_takes_keys(tty)
    }

    fn busctl(args: &[String]) -> Result<(), String> {
        let out = Command::new("busctl")
            .args(args)
            .output()
            .map_err(|e| format!("busctl: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }

    /// The sessions' processes in a systemd user scope with a CPU cap.
    #[derive(Default)]
    pub struct Slower {
        capped: bool,
    }

    impl Slower {
        pub fn slow(&mut self, pids: &[u32]) -> Result<usize, String> {
            let quota = quota_usec(std::thread::available_parallelism().map_or(2, |n| n.get()));
            let outside: Vec<u32> = pids
                .iter()
                .copied()
                .filter(|p| !fs::read_to_string(format!("/proc/{p}/cgroup")).is_ok_and(|c| in_scope(&c)))
                .collect();
            if !outside.is_empty() && busctl(&attach_args(&outside)).is_err() {
                // No scope yet (or it ended with its last process).
                busctl(&start_args(&outside, quota))
                    .map_err(|e| format!("systemd ({e})"))?;
                self.capped = true;
            }
            if !self.capped {
                busctl(&quota_args(quota)).map_err(|e| format!("systemd ({e})"))?;
                self.capped = true;
            }
            Ok(pids.len())
        }

        /// Lifts the cap, whether this desk set it or one before it did.
        pub fn release(&mut self, _pids: &[u32]) {
            let _ = busctl(&quota_args(u64::MAX));
            self.capped = false;
        }

        /// Nothing to do: what is spared is never moved into the scope. (A
        /// process an older desk put there stays capped until it cools down.)
        pub fn spare(&mut self, _pids: &[u32]) {}
    }
}

#[cfg(windows)]
mod os {
    use super::hottest;
    use std::ffi::c_void;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Performance::{
        PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW,
        PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        PROCESS_POWER_THROTTLING_STATE, PROCESS_SET_INFORMATION, ProcessPowerThrottling, SetProcessInformation,
    };
    use windows::core::{PCWSTR, w};

    /// The "Thermal Zone Information" counter, in kelvin.
    pub struct Sensor {
        query: PDH_HQUERY,
        counter: Option<PDH_HCOUNTER>,
    }

    impl Sensor {
        pub fn new() -> Self {
            let mut query = PDH_HQUERY::default();
            let mut counter = PDH_HCOUNTER::default();
            // SAFETY: out-pointers to locals; the query is closed in Drop.
            unsafe {
                if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != 0 {
                    return Self { query, counter: None };
                }
                let ok = PdhAddEnglishCounterW(query, w!("\\Thermal Zone Information(*)\\Temperature"), 0, &mut counter) == 0;
                Self { query, counter: ok.then_some(counter) }
            }
        }

        pub fn read(&mut self) -> Option<f32> {
            let counter = self.counter?;
            // SAFETY: the buffer is sized by PDH's first call and outlives the second.
            unsafe {
                if PdhCollectQueryData(self.query) != 0 {
                    return None;
                }
                let (mut size, mut count) = (0u32, 0u32);
                let _ = PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, None);
                if size == 0 {
                    return None;
                }
                let n = size as usize / size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>() + 1;
                let mut items = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); n];
                if PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, Some(items.as_mut_ptr())) != 0 {
                    return None;
                }
                hottest(
                    items[..(count as usize).min(n)]
                        .iter()
                        .filter(|i| i.FmtValue.CStatus == 0)
                        .map(|i| (i.FmtValue.Anonymous.doubleValue - 273.15) as f32),
                )
            }
        }
    }

    impl Drop for Sensor {
        fn drop(&mut self) {
            // SAFETY: opened in new().
            unsafe {
                let _ = PdhCloseQuery(self.query);
            }
        }
    }

    pub fn pressure() -> bool {
        false
    }

    pub fn processes() -> Vec<super::Proc> {
        let mut out = Vec::new();
        // SAFETY: the snapshot handle is closed below; the entry is sized.
        unsafe {
            let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return out;
            };
            let mut e = PROCESSENTRY32W {
                dwSize: size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            if Process32FirstW(snap, &mut e).is_ok() {
                loop {
                    let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                    out.push(super::Proc {
                        pid: e.th32ProcessID,
                        parent: e.th32ParentProcessID,
                        name: super::program_name(&String::from_utf16_lossy(&e.szExeFile[..len])),
                        tty: None,
                    });
                    if Process32NextW(snap, &mut e).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
        }
        out
    }

    /// Windows does not say what a console is doing: the program a shell
    /// runs always counts as typed into, only what it starts is slowed.
    pub fn typing(_tty: Option<&str>) -> bool {
        true
    }

    #[derive(Default)]
    pub struct Slower;

    impl Slower {
        pub fn slow(&mut self, pids: &[u32]) -> Result<usize, String> {
            Ok(pids.iter().filter(|&&p| eco(p, true)).count())
        }

        pub fn release(&mut self, pids: &[u32]) {
            for &p in pids {
                eco(p, false);
            }
        }

        pub fn spare(&mut self, pids: &[u32]) {
            self.release(pids);
        }
    }

    /// EcoQoS on (efficiency mode) or back to the system's default.
    fn eco(pid: u32, on: bool) -> bool {
        let mask = if on { PROCESS_POWER_THROTTLING_EXECUTION_SPEED } else { 0 };
        let state = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: mask,
            StateMask: mask,
        };
        // SAFETY: the handle is closed; `state` outlives the call.
        unsafe {
            let Ok(h) = OpenProcess(PROCESS_SET_INFORMATION, false, pid) else {
                return false;
            };
            let ok = SetProcessInformation(
                h,
                ProcessPowerThrottling,
                (&raw const state).cast::<c_void>(),
                size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            )
            .is_ok();
            let _ = CloseHandle(h);
            ok
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(p: &mut Policy, readings: &[f32], limit: u32, start: Instant, every: u64) -> Vec<bool> {
        readings
            .iter()
            .enumerate()
            .map(|(i, &c)| p.step(Some(c), false, limit, start + Duration::from_secs(i as u64 * every)))
            .collect()
    }

    #[test]
    fn slows_when_the_average_reaches_the_limit_not_on_one_spike() {
        let mut p = Policy::default();
        let t = Instant::now();
        assert_eq!(run(&mut p, &[60.0, 95.0, 60.0, 60.0], 75, t, 5), [false, false, false, false]);
        let mut p = Policy::default();
        assert_eq!(run(&mut p, &[74.0, 76.0, 76.0], 75, t, 5), [false, false, true]);
    }

    #[test]
    fn full_speed_back_only_well_below_the_limit_and_after_a_minute() {
        let mut p = Policy::default();
        let t = Instant::now();
        assert_eq!(run(&mut p, &[80.0, 80.0, 80.0], 75, t, 5), [false, false, true]);
        // Under the limit but not 7 °C under: still slowed.
        let s = |p: &mut Policy, c, secs| p.step(Some(c), false, 75, t + Duration::from_secs(secs));
        for secs in [15, 20, 25] {
            assert!(s(&mut p, 70.0, secs));
        }
        // Cool, but within the first minute.
        for secs in [30, 35, 40] {
            assert!(s(&mut p, 60.0, secs));
        }
        assert!(!s(&mut p, 60.0, 70));
    }

    #[test]
    fn the_system_saying_too_hot_slows_without_a_sensor() {
        let mut p = Policy::default();
        let t = Instant::now();
        assert!(p.step(None, true, 75, t));
        assert!(p.step(None, false, 75, t + Duration::from_secs(30)));
        assert!(!p.step(None, false, 75, t + Duration::from_secs(61)));
    }

    #[test]
    fn limit_zero_never_slows() {
        let mut p = Policy::default();
        let t = Instant::now();
        assert_eq!(run(&mut p, &[99.0, 99.0, 99.0], 0, t, 5), [false, false, false]);
        assert!(!p.step(None, true, 0, t));
        // Turned off while slowed: back at once.
        assert!(p.step(Some(99.0), false, 75, t));
        assert!(!p.step(Some(99.0), false, 0, t));
    }

    #[test]
    fn modes_set_the_two_limits() {
        assert_eq!(limits(Mode::Off, 75, 70), (0, 0));
        assert_eq!(limits(Mode::Slow, 75, 70), (75, 0));
        assert_eq!(limits(Mode::FansFirst, 75, 70), (75, 70));
        assert_eq!(limits(Mode::Together, 75, 70), (75, 75));
        assert_eq!(limits(Mode::Fans, 75, 70), (0, 75));
        // The fans never wait longer than the slowing, and stay in range.
        assert_eq!(limits(Mode::FansFirst, 75, 80), (75, 75));
        assert_eq!(limits(Mode::FansFirst, 75, 10), (75, 50));
    }

    #[test]
    fn without_fans_keeps_what_a_mode_did_besides() {
        assert_eq!(
            [Mode::Off, Mode::Slow, Mode::FansFirst, Mode::Together, Mode::Fans].map(Mode::without_fans),
            [Mode::Off, Mode::Slow, Mode::Slow, Mode::Slow, Mode::Off]
        );
    }

    #[test]
    fn mode_names() {
        for (name, mode) in [
            ("off", Mode::Off),
            ("slow", Mode::Slow),
            ("fansFirst", Mode::FansFirst),
            ("together", Mode::Together),
            ("fans", Mode::Fans),
        ] {
            assert_eq!(Mode::parse(name), mode);
        }
        assert_eq!(Mode::parse("x"), Mode::Slow);
    }

    #[test]
    fn fan_and_slow_policies_are_independent() {
        let t = Instant::now();
        let (mut fans, mut slow) = (Policy::default(), Policy::default());
        assert_eq!(run(&mut fans, &[72.0, 72.0, 72.0], 70, t, 5), [false, false, true]);
        assert_eq!(run(&mut slow, &[72.0, 72.0, 72.0], 75, t, 5), [false, false, false]);
    }

    fn proc(pid: u32, parent: u32, name: &str, tty: Option<&str>) -> Proc {
        Proc {
            pid,
            parent,
            name: name.into(),
            tty: tty.map(String::from),
        }
    }

    #[test]
    fn descendants_of_live_sessions_only() {
        let table: Vec<Proc> = [(1, 0), (10, 1), (11, 10), (12, 11), (13, 10), (20, 1), (21, 20)]
            .iter()
            .map(|&(pid, parent)| proc(pid, parent, "x", None))
            .collect();
        assert_eq!(descendants(&[10], &table), [10, 11, 12, 13]);
        // A session gone (99) adds nothing; init and desk are never in.
        assert_eq!(descendants(&[99, 20], &table), [20, 21]);
        assert_eq!(descendants(&[1], &table), Vec::<u32>::new());
        let me = std::process::id();
        assert_eq!(descendants(&[me], &[proc(me, 1, "desk", None), proc(me + 1, me, "x", None)]), Vec::<u32>::new());
    }

    /// A terminal takes keys one at a time (raw mode) or not.
    fn raw(ttys: &'static [&'static str]) -> impl Fn(Option<&str>) -> bool {
        move |tty| tty.is_some_and(|t| ttys.contains(&t))
    }

    #[test]
    fn the_agent_you_type_into_keeps_full_speed_and_what_it_runs_is_slowed() {
        // A Claude tile as desk starts it: zsh -lc 'claude ...; exec zsh -l'.
        let t = Some("ttys000");
        let table = [
            proc(1, 0, "launchd", None),
            proc(100, 1, "tmux", None),
            proc(200, 100, "zsh", t),
            proc(210, 200, "claude", t),
            proc(220, 210, "node", t),         // an MCP server
            proc(230, 210, "zsh", None),       // the agent's Bash tool
            proc(231, 230, "node", None),      // pnpm test
            proc(232, 231, "node", None),      // a vitest worker
        ];
        let s = split(&[200], &table, raw(&["ttys000"]));
        assert_eq!(s.spare, [200, 210]);
        assert_eq!(s.slow, [220, 230, 231, 232]);
    }

    #[test]
    fn a_shell_and_the_editor_in_it_keep_full_speed() {
        let t = Some("ttys001");
        let table = [
            proc(300, 1, "zsh", t),
            proc(310, 300, "nvim", t),
            proc(311, 310, "node", None), // a language server
        ];
        let s = split(&[300], &table, raw(&["ttys001"]));
        assert_eq!(s.spare, [300, 310]);
        assert_eq!(s.slow, [311]);
        // The shell alone at its prompt.
        assert_eq!(split(&[300], &table[..1], raw(&["ttys001"])), Split { slow: vec![], spare: vec![300] });
    }

    #[test]
    fn tests_started_from_a_shell_are_slowed_whole() {
        // `pnpm test` typed at a prompt: the terminal is not taking single
        // keys, nobody types into pnpm.
        let t = Some("ttys002");
        let table = [
            proc(400, 1, "zsh", t),
            proc(410, 400, "node", t),
            proc(411, 410, "node", t),
            proc(420, 400, "zsh", t), // a subshell: still a shell
            proc(421, 420, "cargo", t),
        ];
        let s = split(&[400], &table, raw(&[]));
        assert_eq!(s.spare, [400, 420]);
        assert_eq!(s.slow, [410, 411, 421]);
    }

    #[test]
    fn a_session_that_is_one_program_follows_its_terminal() {
        let table = [proc(500, 1, "htop", Some("pts/3")), proc(600, 1, "node", Some("pts/4")), proc(601, 600, "node", Some("pts/4"))];
        let s = split(&[500, 600, 999], &table, raw(&["pts/3"]));
        assert_eq!(s.spare, [500]);
        assert_eq!(s.slow, [600, 601]);
    }

    #[test]
    fn reads_ps() {
        let out = "    1     0 ??       /sbin/launchd\n 2299  2297 ttys000  -zsh\nbad line\n 2319  2299 ttys000  /Users/x/Application Support/bin/claude\n   77     1 ?        kworker\n  900   412 pts/3    bash\n";
        assert_eq!(
            parse_ps(out),
            [
                proc(1, 0, "launchd", None),
                proc(2299, 2297, "zsh", Some("ttys000")),
                proc(2319, 2299, "claude", Some("ttys000")),
                proc(77, 1, "kworker", None),
                proc(900, 412, "bash", Some("pts/3")),
            ]
        );
        assert_eq!(program_name("C:\\Program Files\\PowerShell\\7\\pwsh.exe"), "pwsh");
    }

    #[test]
    fn hottest_skips_dead_sensors() {
        // Apple Silicon's "tdev" sensors read about -9200 °C.
        assert_eq!(hottest([72.5, -9201.1, 86.3, 0.0, 400.0]), Some(86.3));
        assert_eq!(hottest([]), None);
        assert!(is_cpu_sensor("PMU tdie1") && is_cpu_sensor("PMU tdie14"));
        assert!(!is_cpu_sensor("PMU tdev1") && !is_cpu_sensor("gas gauge battery") && !is_cpu_sensor("NAND CH0 temp"));
    }

    #[test]
    fn linux_prefers_cpu_drivers_over_zones() {
        let hwmon = vec![
            ("nvme".to_string(), vec![41_000]),
            ("coretemp".to_string(), vec![61_000, 67_500]),
            ("iwlwifi_1".to_string(), vec![90_000]),
        ];
        let zones = vec![("acpitz".to_string(), 27_800), ("x86_pkg_temp".to_string(), 66_000)];
        assert_eq!(pick_linux(&hwmon, &zones), Some(67.5));
        assert_eq!(pick_linux(&hwmon[..1], &zones), Some(66.0));
        assert_eq!(pick_linux(&[], &[]), None);
    }

    /// `cargo test reads_this_computer -- --ignored --nocapture`
    #[test]
    #[ignore = "reads this computer's sensors"]
    fn reads_this_computer() {
        let c = os::Sensor::new().read();
        println!("CPU: {c:?} °C, pressure: {}", os::pressure());
        assert!(c.is_some());
    }

    /// What desk would slow and spare right now in the sessions whose root
    /// pids are in DESK_ROOTS (`tmux -L desk list-panes -a -F '#{pane_pid}'`):
    /// `DESK_ROOTS="$(...)" cargo test splits_this_computer -- --ignored --nocapture`
    #[cfg(unix)]
    #[test]
    #[ignore = "reads this computer's processes"]
    fn splits_this_computer() {
        let roots: Vec<u32> = std::env::var("DESK_ROOTS")
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(|p| p.parse().ok())
            .collect();
        let table = os::processes();
        let parts = split(&roots, &table, os::typing);
        let show = |pids: &[u32]| {
            let mut names: HashMap<&str, usize> = HashMap::new();
            for p in table.iter().filter(|p| pids.contains(&p.pid)) {
                *names.entry(&p.name).or_default() += 1;
            }
            let mut names: Vec<_> = names.into_iter().collect();
            names.sort();
            format!("{names:?}")
        };
        println!("spared {}: {}", parts.spare.len(), show(&parts.spare));
        println!("slowed {}: {}", parts.slow.len(), show(&parts.slow));
        assert!(!roots.is_empty() && !parts.spare.is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn slows_a_process_and_gives_it_its_speed_back() {
        let mut child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        // getpriority() only reports this for the calling process; ps shows
        // the scheduling priority: 31 normally, 4 in the background.
        let background = || {
            let out = std::process::Command::new("ps")
                .args(["-o", "pri=", "-p", &pid.to_string()])
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).trim() == "4"
        };
        let mut s = os::Slower;
        // Started from a session desk is slowing right now, it inherits that.
        s.release(&[pid]);
        assert!(!background());
        assert_eq!(s.slow(&[pid]), Ok(1));
        assert!(background());
        s.release(&[pid]);
        assert!(!background());
        assert_eq!(s.slow(&[pid]), Ok(1));
        s.spare(&[pid]);
        assert!(!background());
        let _ = child.kill();
        let _ = child.wait();
    }

    /// A real terminal: line mode as a job leaves it, then raw as an editor sets it.
    #[cfg(unix)]
    #[test]
    fn tells_a_terminal_taking_keys_from_one_running_a_job() {
        let (mut master, mut slave) = (0, 0);
        // SAFETY: openpty fills the two descriptors, closed below.
        unsafe {
            assert_eq!(libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()), 0);
            let name = std::ffi::CStr::from_ptr(libc::ttyname(slave)).to_string_lossy().into_owned();
            let tty = name.strip_prefix("/dev/").unwrap();
            assert!(!tty_takes_keys(Some(tty)));
            let mut t: libc::termios = std::mem::zeroed();
            libc::tcgetattr(slave, &mut t);
            libc::cfmakeraw(&mut t);
            libc::tcsetattr(slave, libc::TCSANOW, &t);
            assert!(tty_takes_keys(Some(tty)));
            libc::close(slave);
            libc::close(master);
        }
        assert!(!tty_takes_keys(None));
    }

    #[test]
    fn systemd_calls() {
        let head = "--user call org.freedesktop.systemd1 /org/freedesktop/systemd1 org.freedesktop.systemd1.Manager";
        assert_eq!(
            start_args(&[10, 11], 4_000_000).join(" "),
            format!("{head} StartTransientUnit ssa(sv)a(sa(sv)) desk-sessions.scope fail 2 PIDs au 2 10 11 CPUQuotaPerSecUSec t 4000000 0")
        );
        let attach = attach_args(&[12]);
        assert_eq!(attach[6..], ["ssau", "desk-sessions.scope", "", "1", "12"]);
        assert_eq!(
            quota_args(u64::MAX).join(" "),
            format!("{head} SetUnitProperties sba(sv) desk-sessions.scope true 1 CPUQuotaPerSecUSec t 18446744073709551615")
        );
        assert_eq!([1, 2, 8, 18].map(quota_usec), [1_000_000, 1_000_000, 4_000_000, 9_000_000]);
        assert!(in_scope("0::/user.slice/user-1000.slice/user@1000.service/desk-sessions.scope\n"));
        assert!(!in_scope("0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-desk.scope\n"));
    }
}
