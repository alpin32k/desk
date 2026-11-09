//! Temperature guard: every 5 s desk reads the CPU temperature, shows it in
//! the top bar and, while the computer stays hotter than the limit set in
//! Options (75 °C by default), slows the programs running in its sessions
//! (tests, builds, agents) until it cools down. They keep running, only
//! slower, and nothing outside desk's sessions is touched.
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

use serde::Serialize;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

pub const DEFAULT_LIMIT: u32 = 75;
/// Lowest and highest limit Options accepts; 0 turns slowing off.
pub const MIN_LIMIT: u32 = 50;
pub const MAX_LIMIT: u32 = 100;
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
    /// From Options; 0: never slow.
    pub limit: u32,
    /// Too hot: desk is slowing its sessions.
    pub slowed: bool,
    /// Processes slowed (the sessions and everything they started).
    pub processes: usize,
    /// Why desk cannot slow anything here, if it cannot.
    pub cannot_slow: Option<String>,
}

/// 0 stays 0 (off); anything else within MIN_LIMIT..=MAX_LIMIT.
pub fn clamp_limit(limit: u32) -> u32 {
    if limit == 0 {
        0
    } else {
        limit.clamp(MIN_LIMIT, MAX_LIMIT)
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

/// The given processes still running and everything they started, from a
/// (pid, parent pid) table.
pub fn descendants(roots: &[u32], table: &[(u32, u32)]) -> Vec<u32> {
    let alive: HashSet<u32> = table.iter().map(|&(pid, _)| pid).collect();
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for &(pid, parent) in table {
        if pid != parent {
            children.entry(parent).or_default().push(pid);
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

/// `ps -A -o pid=,ppid=` output.
pub fn parse_ps(out: &str) -> Vec<(u32, u32)> {
    out.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
        })
        .collect()
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
    os: os::Slower,
    /// The first reading after start: put back what a crashed desk left slowed.
    first: bool,
    /// desk is quitting: nothing more is slowed.
    stopped: bool,
}

pub struct Guard {
    limit: AtomicU32,
    inner: Mutex<Inner>,
}

impl Guard {
    pub fn new(limit: u32) -> Self {
        let limit = clamp_limit(limit);
        Self {
            limit: AtomicU32::new(limit),
            inner: Mutex::new(Inner {
                status: Status {
                    celsius: None,
                    limit,
                    slowed: false,
                    processes: 0,
                    cannot_slow: None,
                },
                policy: Policy::default(),
                // A unit struct on macOS and Windows, with state on Linux.
                #[allow(clippy::default_constructed_unit_structs)]
                os: os::Slower::default(),
                first: true,
                stopped: false,
            }),
        }
    }

    pub fn set_limit(&self, limit: u32) {
        self.limit.store(clamp_limit(limit), Ordering::Relaxed);
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
        let limit = self.limit.load(Ordering::Relaxed);
        let was = g.status.slowed;
        let slow = g.policy.step(celsius, pressure, limit, Instant::now());
        let mut next = Status {
            celsius: celsius.map(|c| c.round() as i32),
            limit,
            slowed: slow,
            processes: 0,
            cannot_slow: g.status.cannot_slow.clone(),
        };
        if slow {
            let pids = descendants(&roots(), &os::processes());
            match g.os.slow(&pids) {
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

    /// desk quits: the sessions (tmux keeps them running) get full speed back.
    pub fn shutdown(&self, roots: &[u32]) {
        let mut g = self.inner.lock().unwrap();
        g.stopped = true;
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

#[cfg(unix)]
fn ps_processes() -> Vec<(u32, u32)> {
    std::process::Command::new("ps")
        .args(["-A", "-o", "pid=,ppid="])
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

    pub fn processes() -> Vec<(u32, u32)> {
        super::ps_processes()
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

    pub fn processes() -> Vec<(u32, u32)> {
        super::ps_processes()
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

    pub fn processes() -> Vec<(u32, u32)> {
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
                    out.push((e.th32ProcessID, e.th32ParentProcessID));
                    if Process32NextW(snap, &mut e).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
        }
        out
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
    fn limits() {
        assert_eq!([0, 10, 50, 75, 100, 200].map(clamp_limit), [0, 50, 50, 75, 100, 100]);
    }

    #[test]
    fn descendants_of_live_sessions_only() {
        let table = [(1, 0), (10, 1), (11, 10), (12, 11), (13, 10), (20, 1), (21, 20)];
        assert_eq!(descendants(&[10], &table), [10, 11, 12, 13]);
        // A session gone (99) adds nothing; init and desk are never in.
        assert_eq!(descendants(&[99, 20], &table), [20, 21]);
        assert_eq!(descendants(&[1], &table), Vec::<u32>::new());
        let me = std::process::id();
        assert_eq!(descendants(&[me], &[(me, 1), (me + 1, me)]), Vec::<u32>::new());
    }

    #[test]
    fn reads_ps() {
        assert_eq!(parse_ps("    1     0\n  412     1\nbad line\n 9001  412\n"), [(1, 0), (412, 1), (9001, 412)]);
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
        assert!(!background());
        assert_eq!(s.slow(&[pid]), Ok(1));
        assert!(background());
        s.release(&[pid]);
        assert!(!background());
        let _ = child.kill();
        let _ = child.wait();
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
