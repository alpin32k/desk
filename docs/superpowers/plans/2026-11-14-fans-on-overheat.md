# Fans at Full Speed When Hot Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the CPU gets hot, desk can turn every fan to full speed (and back to auto when it cools), and Options get one "When hot" choice of five behaviours that combine this with today's session slowing.

**Architecture:** A tiny root helper `desk-fans` (its own crate, sharing two source files with desk) writes the SMC fan keys; it accepts only `max`, `auto` and `version`. A one-time admin prompt installs it root-owned with a sudoers rule for exactly `max` and `auto`, as coffee's lid mode does for `pmset`. desk itself reads the fans without root, runs a second `thermal::Policy` for the fans next to the slowing one, calls `sudo -n <helper> max|auto`, and keeps a watchdog and a marker file so the fans never stay manual after desk is gone.

**Tech Stack:** Rust (Tauri 2, IOKit FFI, sha2, tempfile in tests), TypeScript, vitest.

**Spec:** `docs/superpowers/specs/2026-11-14-fans-on-overheat-design.md` (Polish). Read it before any task. The UI copy and the safety rules come from it.

## Global Constraints

- **Fans only at the user's explicit request (owner's rule).** By default desk never touches the fans and never installs the helper: not on first launch, not when migrating an old `layout.json`, not on update. Nothing nudges the user toward it. The helper and the sudoers rule come into existence only after the user picks a fan mode in Options and enters the password.

- Worktree `/Users/alpinem3/Projects/desk-dropdown.worktrees/fans`, branch `feat/fans-on-overheat` (from `upstream/main`). Commit subject in English, in the repo's sentence style ("The usage chip is a button…"), body in Polish. No AI attribution anywhere.
- Code and comments in English. UI text only through `src/locales/pl.ts` and `en.ts`. Rust messages through `tr!`, except in the files the helper shares (`fan_control.rs`, `smc.rs`), which use plain English.
- Helper installed at `/Library/PrivilegedHelperTools/pl.makarowski.desk.fans` (`root:wheel`, `0755`). Rule at `/etc/sudoers.d/desk-fans` (`0440`), exactly: `<user> ALL=(root) NOPASSWD: <helper> max, <helper> auto`.
- Helper arguments: `max`, `auto`, `version`. Anything else exits 2 without touching the SMC. `PROTOCOL = 1`.
- desk's files in its support dir: `fans-forced` (marker: desk set the fans) and `fans.stop` (stops the watchdog).
- Defaults: mode `"slow"`, slowing limit 75, fan limit 70 (`DEFAULT_FAN_LIMIT`). Both limits range over 50–100, and the fan limit never goes above the slowing limit in `fansFirst`. Back to auto 7 °C lower (`COOL_MARGIN`), and not before 60 s have passed (`MIN_SLOW`). A forced state lost on the hardware (sleep) is re-applied at most once per 60 s.
- Mode names (layout and TS): `"off" | "slow" | "fansFirst" | "together" | "fans"`. An unknown value falls back to `"slow"`.
- desk never touches fans it did not set: no `max` over another program's manual mode, and no `auto` without its marker.
- Done means: `pnpm check` passes on macOS (typecheck, eslint, vitest, `cargo clippy --all-targets -- -D warnings`, `cargo test`), plus `cargo clippy --manifest-path src-tauri/fans-helper/Cargo.toml -- -D warnings` and `cargo build --release --manifest-path src-tauri/fans-helper/Cargo.toml`.

## Review Focus

1. **The Mac sleeps while the fans are forced.** The SMC may wake in auto mode while desk still thinks it is forcing them. Expected: within a minute desk forces them again. The test is `next_action_reapplies_a_lost_force_once_a_minute` in Task 4.
2. **Another fan tool already holds the fans in manual mode** (Stats, Macs Fan Control). Expected: desk never sends `max` or `auto`, and the chip says "another program". The tests are `next_action_leaves_another_programs_fans_alone` in Task 4 and `recover_without_marker_does_nothing` in Task 4.
3. **desk is killed while the fans are forced** (`kill -9`, crash, reboot). Expected: the watchdog runs `auto` within ~5 s, and a later start with the marker still present runs `auto` once. The tests are `watchdog_restores_auto_only_when_desk_is_gone` in Task 4, plus manual step 8.4.
4. **A half-applied `max` on hardware nobody has tried** (a write rejected mid-way). Expected: the helper ends in auto, never in manual with an old (0) target. The tests are `failed_target_write_restores_auto` and `force_max_gives_up_and_restores_auto` in Task 1.
5. **An old `layout.json`** (`thermalLimit: 0` = slowing off, or a custom limit). Expected: `0` loads as mode `off` with the limit at 75, and any other value as mode `slow` with that limit. The tests are `old_file_with_slowing_off_becomes_mode_off` and `old_file_with_a_limit_keeps_slowing` in Task 5.

---

### Task 1: Fan logic shared by desk and the helper

**Files:**
- Create: `src-tauri/src/fan_control.rs`
- Modify: `src-tauri/src/lib.rs` (`pub mod fan_control;`)
- Test: `src-tauri/src/fan_control.rs` (`mod tests`, with a fake SMC)

**Interfaces:**
- Produces (no `tr!`, no `crate::` paths except `crate::fan_control`, so the helper can include this file with `#[path]`):
  - `pub const PROTOCOL: u32 = 1; pub const AUTO: u8 = 0; pub const MANUAL: u8 = 1; pub const TRIES: usize = 100;`
  - `pub trait Smc { fn read_u8(&mut self, key: &str) -> Result<u8, String>; fn read_f32(&mut self, key: &str) -> Result<f32, String>; fn write_u8(&mut self, key: &str, value: u8) -> Result<(), String>; fn write_f32(&mut self, key: &str, value: f32) -> Result<(), String>; }`
  - `#[derive(Debug, Clone, PartialEq)] pub struct Fan { pub mode: u8, pub rpm: f32, pub max_rpm: f32 }`
  - `pub fn key(fan: u8, field: &str) -> String` (`key(1, "Mx") == "F1Mx"`)
  - `pub fn read(smc: &mut impl Smc) -> Result<Vec<Fan>, String>`: reads `Ftst` (proves the Mac is fan-controllable) and `FNum` (ui8), then per fan `FnMd` (ui8), `FnAc`, `FnMx` (f32). Returns `Err` when `Ftst` or `FNum` is unreadable, and `Ok(vec![])` when `FNum` is 0.
  - `pub fn manual(fans: &[Fan]) -> bool`: any fan has mode `MANUAL`.
  - `pub fn force_max(smc: &mut impl Smc, pause: &mut dyn FnMut()) -> Result<(), String>`
  - `pub fn restore_auto(smc: &mut impl Smc) -> Result<(), String>`

- [ ] **Step 1: Write the failing tests** with a `FakeSmc { u8s: HashMap<String,u8>, f32s: HashMap<String,f32>, reject: HashSet<String>, md_locked: usize, log: Vec<String> }`. `md_locked` makes the first N writes to any `F?Md` fail. `log` records each successful write as `"F0Tg=6800"`. The start state has two fans: `Ftst 0, FNum 2, F0Md 3, F1Md 3, F0Mx 6800, F1Mx 6800, F0Ac 0, F1Ac 0, F0Tg 0, F1Tg 0`.
  - `key_names`: `key(0,"Md") == "F0Md"`, `key(1,"Mx") == "F1Mx"`.
  - `read_reports_fans_and_manual`: 2 fans, `max_rpm == 6800.0`, and `!manual(..)`. After setting `F1Md = 1`, `manual(..)` is true.
  - `read_without_fan_keys_is_unsupported`: an empty fake gives `Err`. `Ftst 0, FNum 0` gives `Ok(vec![])`.
  - `force_max_sets_target_before_manual_mode_on_every_fan`: `log == ["Ftst=1","F0Tg=6800","F0Md=1","F1Tg=6800","F1Md=1"]`.
  - `force_max_retries_manual_mode_after_unlock`: `md_locked = 3` gives `Ok`, and `pause` is called 3 times.
  - `force_max_gives_up_and_restores_auto`: with `md_locked = usize::MAX` it returns `Err`, `pause` is called `TRIES` times, and the end state has `F0Md == AUTO`, `Ftst == 0`.
  - `failed_target_write_restores_auto`: with `reject = {"F1Tg"}` it returns `Err`, and the end state has `F0Md == AUTO`, `F1Md != MANUAL`, `Ftst == 0`.
  - `restore_auto_tries_every_fan_even_if_one_fails`: with `reject = {"F0Md"}` it returns `Err`, and `F1Md == AUTO`, `Ftst == 0`.
  - `force_max_without_fans_is_an_error`: `FNum 0` gives `Err`, and `log` is empty.

- [ ] **Step 2: Run them, expect failures**

Run: `cd src-tauri && cargo test fan_control`
Expected: compile errors (module missing).

- [ ] **Step 3: Implement.** `force_max` is the safety-critical order, so implement exactly this:

```
fans = read(smc)?; if fans.is_empty() -> Err("no fans")
write_u8("Ftst", 1)                  // error -> restore_auto, Err
for i in 0..fans.len():
    write_f32(key(i,"Tg"), fans[i].max_rpm)        // target BEFORE manual mode
    try up to TRIES: write_u8(key(i,"Md"), MANUAL); on error pause()
    any error -> restore_auto (ignore its result), return Err
verify: read again; every mode == MANUAL and |read_f32(key(i,"Tg")) - max_rpm| < 1.0, else restore_auto, Err("not applied")
```

`restore_auto`: read `FNum`, then write `FnMd = AUTO` for every fan (keep going after a failure and remember the first error), then `Ftst = 0`. It returns the first error, or `Err` if any fan still reads `MANUAL`.

- [ ] **Step 4: Run the tests, expect them to pass**

Run: `cd src-tauri && cargo test fan_control && cargo clippy --all-targets -- -D warnings`
Expected: all pass, no warnings.

- [ ] **Step 5: Commit** with the subject `Fan control logic: max with the target before manual mode, auto on any failure`.

### Task 2: Reading and writing the SMC on macOS

**Files:**
- Create: `src-tauri/src/smc.rs` (macOS only: its `mod` line carries `#[cfg(target_os = "macos")]` in `lib.rs` and in the helper)
- Modify: `src-tauri/src/lib.rs` (`#[cfg(target_os = "macos")] pub mod smc;`)
- Test: `src-tauri/src/smc.rs` (`mod tests`)

**Interfaces:**
- Consumes: `crate::fan_control::{Smc, read}`.
- Produces: `pub struct IoSmc` with `pub fn open() -> Result<IoSmc, String>`, `impl crate::fan_control::Smc for IoSmc`, and `Drop` (closes the connection). Uses only IOKit FFI and `libc` (`mach_task_self`). No `tr!`.

The AppleSMC protocol (not determined by the signatures):
- Open: `IOServiceGetMatchingService(0, IOServiceMatching(c"AppleSMC"))`, then `IOServiceOpen(service, mach_task_self(), 0, &mut conn)`, then `IOObjectRelease(service)`.
- Every call: `IOConnectCallStructMethod(conn, 2, &input, 80, &mut output, &mut 80)`. A non-zero `kern_return` or `output.result != 0` is an error. The text names the key, and for writes says "needs root".
- The struct is `#[repr(C)]`, 80 bytes. Assert `size_of == 80` in a test:
  `key: u32` (the 4 ASCII chars big-endian), `vers: [u8; 6]`, `p_limit: [u16; 2] + [u32; 3]`, `key_info: { data_size: u32, data_type: u32, attributes: u8 }`, `result: u8`, `status: u8`, `data8: u8`, `data32: u32`, `bytes: [u8; 32]`.
- Read: `data8 = 9` gives `key_info`. Then `key_info.data_size = size`, `data8 = 5` gives `bytes`. Write: `data8 = 6`, `key_info.data_size = size`, `bytes` filled.
- Types: `ui8 ` (`0x75693820`, 1 byte) and `flt ` (`0x666c7420`, 4 bytes, little-endian f32). Reading a key with another type is an `Err` ("unexpected type"). That is how an Intel or unknown Mac ends up "unsupported".

- [ ] **Step 1: Write the failing tests**
  - `smc_struct_is_80_bytes`.
  - `reads_this_mac`: if `IoSmc::open()` fails (no SMC, e.g. a VM), return early. Otherwise `fan_control::read` is `Ok`. On this M3 Pro: 2 fans, each `max_rpm > 0`.
  - `writing_without_root_is_refused`: skip if `libc::geteuid() == 0` or `open()` fails. `write_u8("Ftst", 0)` is `Err` (writing 0, today's value, is harmless even if it succeeded).

- [ ] **Step 2: Run them, expect failures**

Run: `cd src-tauri && cargo test smc`
Expected: compile errors.

- [ ] **Step 3: Implement** `IoSmc` as specified above. Keep the FFI declarations local to this file, the way `thermal.rs` declares its own.

- [ ] **Step 4: Run the tests, expect them to pass**

Run: `cd src-tauri && cargo test smc && cargo clippy --all-targets -- -D warnings`
Expected: the 3 tests pass, and `reads_this_mac` really reads 2 fans.

- [ ] **Step 5: Commit** with the subject `desk reads the fans through the SMC without root`.

### Task 3: The `desk-fans` helper and its place in the macOS bundle

**Files:**
- Create: `src-tauri/fans-helper/Cargo.toml`, `src-tauri/fans-helper/main.rs`, `.github/build-fans.sh`, `src-tauri/tauri.mac-bundle.conf.json`
- Modify: `src-tauri/.gitignore` (`/fans-helper/target/`), `.github/workflows/release.yml` (macOS row)

**Interfaces:**
- Consumes: `fan_control::{PROTOCOL, force_max, restore_auto}` and `smc::IoSmc`, included with `#[path = "../src/fan_control.rs"] mod fan_control;` and `#[cfg(target_os = "macos")] #[path = "../src/smc.rs"] mod smc;`.
- Produces: the binary `desk-fans`. `.github/build-fans.sh <triple>` writes it to `src-tauri/binaries/desk-fans-<triple>`, where Tauri's `externalBin` puts it at `desk.app/Contents/MacOS/desk-fans`.

- [ ] **Step 1: Write the crate.**
  - `Cargo.toml`: `name = "desk-fans"`, `edition = "2024"`, `publish = false`, `[[bin]] name = "desk-fans" path = "main.rs"`, `[target.'cfg(target_os = "macos")'.dependencies] libc = "0.2"`, and an empty `[workspace]`, so it never joins `src-tauri`'s build.
  - `main.rs` (`#![allow(dead_code)]`, because the shared files carry more than the helper uses): it reads only `std::env::args().nth(1)`.
    - `version` prints `PROTOCOL` and exits 0.
    - On macOS, `max` and `auto` call `IoSmc::open()` and then `force_max(&mut smc, &mut || sleep(50 ms))` or `restore_auto`. Ok exits 0. Err exits 1 after printing the error on stderr.
    - On other systems, `max` and `auto` print `unsupported` and exit 2.
    - Anything else prints `usage: desk-fans max|auto|version` and exits 2.

- [ ] **Step 2: Build it and check its behaviour without root**

```bash
cargo build --release --manifest-path src-tauri/fans-helper/Cargo.toml
H=src-tauri/fans-helper/target/release/desk-fans
$H version            # prints: 1
$H foo; echo $?       # usage line, 2
$H max; echo $?       # an error naming "needs root", 1
/Applications/Stats.app/Contents/Resources/smc fans | grep Mode   # still auto (no "forced"/1)
cargo clippy --manifest-path src-tauri/fans-helper/Cargo.toml -- -D warnings
```

Expected: exactly as commented. A failed `max` left the fans in auto. That is the rollback path running on real hardware.

- [ ] **Step 3: Bundle it.**
  - `.github/build-fans.sh <triple>` (`set -eu`) runs `cargo build --release --manifest-path src-tauri/fans-helper/Cargo.toml --target "$1"` and copies the result to `src-tauri/binaries/desk-fans-$1`. The header comment follows `build-tmux.sh`.
  - `src-tauri/tauri.mac-bundle.conf.json`: `{"bundle":{"externalBin":["binaries/tmux","binaries/desk-fans"]}}`. Do not name it `tauri.macos.conf.json`: Tauri would merge it into every macOS build, dev included.
  - In `release.yml`, the macOS row gets `fans: true`, a step `sh .github/build-fans.sh ${{ matrix.target }}` (`if: matrix.fans`) before `tauri build`, and its `args` use `--config src-tauri/tauri.mac-bundle.conf.json` in place of `tauri.tmux.conf.json`. The Linux and Windows rows stay unchanged.

- [ ] **Step 4: Verify the bundle locally**

```bash
sh .github/build-fans.sh aarch64-apple-darwin
[ -f src-tauri/binaries/tmux-aarch64-apple-darwin ] || sh .github/build-tmux.sh aarch64-apple-darwin
pnpm tauri build --target aarch64-apple-darwin --bundles app --config src-tauri/tauri.mac-bundle.conf.json
ls src-tauri/target/aarch64-apple-darwin/release/bundle/macos/desk.app/Contents/MacOS/
```

Expected: `desk`, `desk-fans` and `tmux` are listed, and `.../desk-fans version` prints `1`.

- [ ] **Step 5: Commit** with the subject `desk-fans: a root helper that knows only max, auto and version, shipped in the macOS app`.

### Task 4: Permission, scripts and the fan controller on desk's side

**Files:**
- Create: `src-tauri/src/admin.rs`, `src-tauri/src/fans.rs`
- Modify: `src-tauri/src/coffee.rs` (use `admin::*`; move `applescript_strings_are_escaped` to `admin.rs`), `src-tauri/src/lib.rs` (`pub mod admin; pub mod fans;`)
- Test: `src-tauri/src/admin.rs`, `src-tauri/src/fans.rs`

**Interfaces:**
- Consumes: `fan_control::{Fan, read, manual, PROTOCOL}` and `smc::IoSmc` (macOS), `sha2`, `crate::tmux::shell_quote`.
- Produces in `admin.rs` (moved from `coffee.rs`, which calls them unchanged in behaviour):
  - `#[cfg(target_os = "macos")] pub fn run_admin(script: &str, prompt: &str) -> Result<(), String>` (cancel gives `"anulowano"` as today, via `tr!`, `"cancelled"` in English)
  - `pub fn applescript_quote(s: &str) -> String`
  - `pub fn check_user(user: &str) -> Result<(), String>` (today's rule from `coffee::sudoers_rule`)
- Produces in `fans.rs`:
  - `pub const HELPER: &str = "/Library/PrivilegedHelperTools/pl.makarowski.desk.fans"; pub const SUDOERS: &str = "/etc/sudoers.d/desk-fans";`
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "lowercase")] pub enum Consent { Missing, Outdated, Installed }`
  - `#[derive(Debug, Clone, PartialEq, Serialize)] #[serde(rename_all = "camelCase")] pub struct Info { pub unsupported: Option<String>, pub consent: Consent }`
  - `#[derive(Debug, Clone, PartialEq, Serialize)] #[serde(rename_all = "camelCase")] pub struct FanStatus { pub forced: bool, pub rpm: Option<u32>, pub max_rpm: Option<u32>, pub problem: Option<String> }` (`rpm`: the fastest fan's actual speed, rounded to 100)
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Action { Nothing, Force, Release, OtherProgram }`
  - `pub fn consent(sudoers: bool, helper_version: Option<u32>) -> Consent`
  - `pub fn next_action(want: bool, ours: bool, manual: bool, since_force: Option<Duration>) -> Action`
  - `pub fn sudoers_rule(user: &str) -> Result<String, String>`
  - `pub fn install_script(rule: &str, source: &str, sha256: &str) -> String`
  - `pub fn uninstall_script() -> String`
  - `pub fn watchdog_script(pid: u32, stop: &Path, marker: &Path) -> String`
  - `pub struct Fans` with `pub fn new(support_dir: &Path) -> Self`, `pub fn ours(&self) -> bool`, `pub fn recover(&mut self)`, `pub fn step(&mut self, want: bool, now: Instant) -> FanStatus`, `pub fn release(&mut self)`
  - `pub fn info() -> Info`, `pub fn install() -> Result<Info, String>`, `pub fn uninstall() -> Result<Info, String>`

- [ ] **Step 1: Write the failing tests**
  - `admin.rs`: `applescript_strings_are_escaped` (moved), and `check_user` rejects `""`, `"a b"`, `"x ALL=(ALL) ALL"`, `"-x"` and accepts `"kamil"`.
  - `consent_needs_rule_and_current_helper`: `(false, Some(1))` gives `Missing`, `(true, None)` gives `Missing`, `(true, Some(0))` gives `Outdated`, and `(true, Some(PROTOCOL))` gives `Installed`.
  - `next_action_forces_when_hot_and_releases_when_cool`: `(true,false,false,None)` gives `Force`, `(false,true,true,_)` gives `Release`, and `(false,false,false,None)` gives `Nothing`.
  - `next_action_leaves_another_programs_fans_alone`: `(true,false,true,None)` gives `OtherProgram`, and `(false,false,true,None)` gives `Nothing`.
  - `next_action_reapplies_a_lost_force_once_a_minute`: `(true,true,false,Some(30 s))` gives `Nothing`, `(true,true,false,Some(60 s))` gives `Force`, and `(true,true,true,Some(600 s))` gives `Nothing`.
  - `sudoers_rule_allows_exactly_max_and_auto`: `sudoers_rule("kamil") == "kamil ALL=(root) NOPASSWD: /Library/PrivilegedHelperTools/pl.makarowski.desk.fans max, /Library/PrivilegedHelperTools/pl.makarowski.desk.fans auto\n"`, and the bad names from `check_user` give `Err`.
  - `install_script_checks_the_copy_before_installing`: given `source = "/Apps/my desk.app/Contents/MacOS/desk-fans"` and `sha = "ab12…"` (64 hex), the script contains `'/Apps/my desk.app/Contents/MacOS/desk-fans'`, `shasum -a 256`, the sha, `xattr -c`, `install -m 0755 -o root -g wheel`, `'/Library/PrivilegedHelperTools/pl.makarowski.desk.fans'`, `visudo -cf`, `install -m 0440 -o root -g wheel` and `'/etc/sudoers.d/desk-fans'`. The index of the sha check is lower than the index of the helper's `install`.
  - `uninstall_script_removes_both`: it contains `/bin/rm -f` with both quoted paths.
  - `watchdog_restores_auto_only_when_desk_is_gone`: for pid 42 and paths with spaces, the script contains `/bin/kill -0 42` and `[ ! -e '<stop>' ]`, and ends with `/bin/kill -0 42 2>/dev/null || { /usr/bin/sudo -n /Library/PrivilegedHelperTools/pl.makarowski.desk.fans auto; /bin/rm -f '<marker>'; }`.
  - `recover_without_marker_does_nothing`: `Fans::new(tmp)` and then `recover()` in an empty temp dir leaves `ours() == false` and creates no files (no helper call is possible, because there is no marker).

- [ ] **Step 2: Run them, expect failures**

Run: `cd src-tauri && cargo test fans:: admin:: coffee::`
Expected: compile errors.

- [ ] **Step 3: Implement.**
  - `install_script` runs as root via `run_admin`, under `set -e`: `mktemp -d` (root-owned) and a `trap` that removes it, `cp` the source in, compare `shasum -a 256` with the given sha (mismatch prints to stderr and exits 1), `xattr -c`, `install -d -m 0755 -o root -g wheel /Library/PrivilegedHelperTools`, install the helper `0755 root:wheel`, `printf` the rule into the temp dir, run `visudo -cf` on it, and install it `0440 root:wheel`. Quote every path with `shell_quote`.
  - `install()` (macOS):
    - Find the source with `bundled_helper()`: `current_exe().parent()/desk-fans`, and only in `cfg!(debug_assertions)` also `concat!(env!("CARGO_MANIFEST_DIR"), "/binaries/desk-fans-aarch64-apple-darwin")`. None gives `Err(tr!("w tej paczce desk nie ma desk-fans", "this desk build has no desk-fans"))`.
    - Hash the source with sha2 (hex as in `accounts.rs`), check `$USER`, and call `run_admin(script, prompt)`.
    - The prompt is `tr!("desk: jednorazowa zgoda na sterowanie wiatrakami przy przegrzaniu (tylko „maksimum” i „auto”). Później desk nie będzie już pytać.", "desk: one-time permission to control the fans when hot (only “max” and “auto”). desk will not ask again.")`.
    - It returns `info()`.
  - `uninstall()` runs `run_admin(uninstall_script(), tr!("desk: usunięcie zgody na sterowanie wiatrakami.", "desk: removing the fan permission."))`. Elsewhere it is `Ok(info())`.
  - `info()`: `consent(Path::new(SUDOERS).exists(), helper_version())`, where `helper_version()` runs `HELPER version` without sudo. `unsupported` is set as follows:
    - non-macOS: `tr!("tylko macOS na Apple Silicon", "macOS on Apple Silicon only")`;
    - `IoSmc::open()` or `fan_control::read` fails: the same text;
    - no fans: `tr!("ten Mac nie ma wiatraków", "this Mac has no fans")`.
  - `Fans::step`:
    - Read the fans. If that fails, return `FanStatus { problem: Some(unsupported text) if want, .. }`.
    - Compute `next_action(want, self.ours, manual(&fans), since last force)`.
    - `Force`:
      - Check `info().consent`. `Missing` gives `tr!("brak zgody (Opcje → Zasilanie)", "no permission (Options → Power)")`. `Outdated` gives `tr!("zgoda z poprzedniej wersji desk (Opcje → Zasilanie)", "permission from an older desk (Options → Power)")`.
      - Otherwise run `/usr/bin/sudo -n HELPER max`. Its stderr becomes the problem.
      - On success: `ours = true`, `last_force = now`, write the marker, remove `fans.stop`, and spawn the watchdog once per forced period. Spawn it as `sh -c` with `CommandExt::process_group(0)`, so Ctrl+C in `tauri dev` does not kill it.
    - `Release`: `release()`.
    - `OtherProgram`: problem `tr!("wiatrakami steruje inny program", "another program controls the fans")`.
    - Read again for `rpm`.
    - Put a problem in the status only while `want`.
  - `release()`: if `ours`, run `sudo -n HELPER auto`, remove the marker, write `fans.stop`, and set `ours = false`.
  - `recover()`: if the marker exists and `info().consent == Installed`, run `sudo -n HELPER auto`. Remove the marker either way.

- [ ] **Step 4: Run the tests, expect them to pass**

Run: `cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings`
Expected: all pass. The coffee tests still pass after the move.

- [ ] **Step 5: Commit** with the subject `desk installs the fan helper once and drives it with a watchdog and a marker`.

### Task 5: Modes in the temperature guard, layout and commands

**Files:**
- Modify: `src-tauri/src/thermal.rs`, `src-tauri/src/layout.rs`, `src-tauri/src/lib.rs`
- Test: `src-tauri/src/thermal.rs`, `src-tauri/src/layout.rs`

**Interfaces:**
- Consumes: `fans::{Fans, FanStatus, Info, info, install, uninstall}`.
- Produces:
  - `thermal.rs`: `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Mode { Off, Slow, FansFirst, Together, Fans }` with `pub fn parse(s: &str) -> Mode`, `pub const DEFAULT_FAN_LIMIT: u32 = 70;`, and `pub fn limits(mode: Mode, limit: u32, fan_limit: u32) -> (u32, u32)` (slow, fans; 0 = unused).
  - `Status` gains `pub fan_limit: u32` and `pub fans: Option<FanStatus>`. `limit` now means the slowing limit, 0 when the mode does not slow. `fans` is `None` when the mode does not use them.
  - `Guard::new(mode: Mode, limit: u32, fan_limit: u32, support_dir: &Path)`, and `Guard::configure(&self, mode: Mode, limit: u32, fan_limit: u32)` replacing `set_limit`. `Guard::release_fans(&self)`. `tick` and `shutdown` keep their signatures.
  - `Layout` gains `thermal_mode: String` (default `"slow"`) and `fan_limit: u32` (default `DEFAULT_FAN_LIMIT`).
  - Commands: `fans_info() -> fans::Info`, `async fans_install() -> Result<fans::Info, String>`, `async fans_uninstall(guard) -> Result<fans::Info, String>`. Both async commands run the blocking part in `tauri::async_runtime::spawn_blocking`, and `fans_uninstall` calls `guard.release_fans()` first.

- [ ] **Step 1: Write the failing tests**
  - `modes_set_the_two_limits`:
    - `limits(Off,75,70) == (0,0)`, `limits(Slow,75,70) == (75,0)`, `limits(FansFirst,75,70) == (75,70)`, `limits(Together,75,70) == (75,75)`, `limits(Fans,75,70) == (0,75)`.
    - In `FansFirst`, a fan limit above the slowing limit is pulled down to it: `(FansFirst,75,80) == (75,75)`.
    - A fan limit below the range is raised to it: `(FansFirst,75,10) == (75,50)`.
  - `mode_names`: `parse` of each of `"off","slow","fansFirst","together","fans"` round-trips, and `parse("x") == Slow`.
  - `fan_and_slow_policies_are_independent`: two `Policy`s, readings `[72,72,72]`. The policy with limit 70 gives `true`, the one with limit 75 gives `false`.
  - `layout.rs`:
    - `old_file_with_slowing_off_becomes_mode_off`: `{"thermalLimit":0}` loads as `thermal_mode "off"`, `thermal_limit 75`, `fan_limit 70`.
    - `old_file_with_a_limit_keeps_slowing`: `{"thermalLimit":80}` loads as `"slow"`, `80`.
    - `round_trip` sets `thermal_mode: "fansFirst"`, `fan_limit: 65`.

- [ ] **Step 2: Run them, expect failures**

Run: `cd src-tauri && cargo test thermal layout`
Expected: compile errors.

- [ ] **Step 3: Implement.**
  - Migration in `layout::load`: when `thermal_limit == 0`, set `thermal_mode = "off"` and `thermal_limit = DEFAULT_LIMIT`. The new format never stores 0.
  - Store `Mode`, `limit` and `fan_limit` in a `Mutex` next to the existing state, in place of the `AtomicU32`.
  - `Inner` gains `fan_policy: Policy` and `fans: Fans`.
  - `tick`:
    - `let (slow_limit, fan_limit) = limits(..)`. The slowing path uses `slow_limit`.
    - `want = fan_policy.step(celsius, pressure, fan_limit, now)`.
    - On the first tick, call `fans.recover()`.
    - If `fan_limit > 0 || fans.ours()`, call `fans.step(want, now)`. `status.fans` is that result only when `fan_limit > 0`.
  - `shutdown` also calls `fans.release()`.
  - `lib.rs`:
    - Pass `Mode::parse(&saved.thermal_mode)`, `saved.thermal_limit`, `saved.fan_limit` and `&support` to `Guard::new`.
    - `save_layout` calls `configure`.
    - Register the three commands.

- [ ] **Step 4: Run the tests, expect them to pass**

Run: `cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings`
Expected: all pass.

- [ ] **Step 5: Commit** with the subject `The temperature guard has five modes: fans, slowing, both, one before the other, or nothing`.

### Task 6: Chip, title, toasts and mode list in the frontend logic

**Files:**
- Modify: `src/api.ts`, `src/thermal.ts`, `src/locales/pl.ts`, `src/locales/en.ts`, `src/app.ts` (only the boot defaults for the two new `Layout` keys, so the typecheck passes)
- Test: `src/thermal.test.ts`

**Interfaces:**
- Consumes: the Rust `Status`, `FanStatus` and `Info` (camelCase).
- Produces:
  - `api.ts`:
    - `export type ThermalMode = "off" | "slow" | "fansFirst" | "together" | "fans";`
    - `Layout` gains `thermalMode: ThermalMode; fanLimit: number;`.
    - `export interface FanStatus { forced: boolean; rpm: number | null; maxRpm: number | null; problem: string | null }`.
    - `ThermalStatus` gains `fanLimit: number; fans: FanStatus | null`.
    - `export interface FanInfo { unsupported: string | null; consent: "missing" | "outdated" | "installed" }`.
    - `api.fansInfo()`, `api.fansInstall()`, `api.fansUninstall()`, all returning `FanInfo`.
  - `thermal.ts`:
    - `DEFAULT_FAN_LIMIT = 70`, `THERMAL_MODES: ThermalMode[]` (in spec order: off, slow, fansFirst, together, fans).
    - `usesFans(mode): boolean`, `clampFanLimit(n: number, limit: number): number`.
    - `modeOptions(info: FanInfo | null): { mode: ThermalMode; why: string | null }[]` (`why` set: the fan mode is disabled).
    - `thermalToast(prev: ThermalStatus | null, next: ThermalStatus): string | null`.
    - The existing `thermalHtml`, `thermalTitle` and `thermalLevel` are extended.

Copy (Polish from the spec; English as given):

| key | pl | en |
|---|---|---|
| `settings.thermalMode` | Przy przegrzaniu | When hot |
| `settings.thermalModeOff` | Nic | Nothing |
| `settings.thermalModeSlow` | Zwalniaj sesje | Slow sessions |
| `settings.thermalModeFansFirst` | Wiatraki, potem zwalnianie | Fans, then slowing |
| `settings.thermalModeTogether` | Wiatraki i zwalnianie | Fans and slowing |
| `settings.thermalModeFans` | Tylko wiatraki | Fans only |
| `settings.fanLimit` | Próg wiatraków | Fans threshold |
| `settings.fansInstalled` | Zgoda na wiatraki jest zainstalowana (tylko „maksimum” i „auto”). | Fan permission is installed (only “max” and “auto”). |
| `settings.fansRemove` | Usuń zgodę | Remove permission |
| `settings.fansAsk` | Tryby z wiatrakami raz zapytają o hasło administratora. | Fan modes ask for the admin password once. |
| `settings.fansOutdated` | Zgoda na wiatraki jest z poprzedniej wersji desk. | The fan permission is from an older desk. |
| `settings.fansUpdate` | Zaktualizuj zgodę | Update permission |
| `settings.thermalAboutOff` | desk tylko pokazuje temperaturę. | desk only shows the temperature. |
| `settings.thermalAboutFans` | Wszystkie wiatraki idą na pełne obroty, a 7°C niżej wracają do auto, czyli pod kontrolę macOS. Gdy wiatrakami steruje inny program (Stats, Macs Fan Control), desk ich nie rusza. | Every fan goes to full speed and returns to auto, macOS's own control, 7°C lower. If another program controls the fans (Stats, Macs Fan Control), desk leaves them alone. |
| `fans.error` | Wiatraki: {err} | Fans: {err} |
| `fans.unknown` | nie udało się sprawdzić wiatraków | could not check the fans |
| `thermal.offTitle` (changed) | Procesor: {c}°C. Przy przegrzaniu desk nic nie robi (Opcje). | CPU: {c}°C. desk does nothing when hot (Options). |
| `thermal.cpu` | Procesor: {c}°C. | CPU: {c}°C. |
| `thermal.fansReady` | Powyżej {limit}°C wiatraki idą na maksimum. | Above {limit}°C the fans go to max. |
| `thermal.fansForced` | Wiatraki na maksimum ({rpm} obr./min), wrócą do auto przy {cool}°C. | Fans at max ({rpm} rpm), back to auto at {cool}°C. |
| `thermal.fansProblemLine` | desk nie steruje wiatrakami: {why} | desk is not controlling the fans: {why} |
| `thermal.fansOn` | Procesor ma {c}°C: wiatraki na maksimum | CPU at {c}°C: fans at max |
| `thermal.fansOff` | Procesor ostygł ({c}°C): wiatraki wracają do auto | CPU cooled down ({c}°C): fans back to auto |
| `thermal.slowingFans` | Procesor ma {c}°C: wiatraki na maksimum, a desk zwalnia programy w sesjach | CPU at {c}°C: fans at max and desk slows the programs in its sessions |
| `thermal.cooledFans` | Procesor ostygł ({c}°C): sesje znowu pełną parą, wiatraki wracają do auto | CPU cooled down ({c}°C): sessions at full speed again, fans back to auto |
| `thermal.fansProblem` | Wiatraki nie ruszyły: {why} | Fans did not start: {why} |

- [ ] **Step 1: Write the failing tests** in `thermal.test.ts`, with an `at(c, extra)` builder like the existing one:
  - `chip shows ≋ while the fans are forced`: `thermalHtml` gives the text `78° ≋` with fans forced, and `81° ≋ ↓` with fans forced plus slowing (`processes > 0`).
  - `level is hot with forced fans and warm relative to the lowest limit`: `{limit: 0, fanLimit: 70, celsius: 61}` gives `"warm"`, and `fans.forced` gives `"hot"`.
  - `title names rpm and the auto temperature`: forced with `rpm 6800`, `fanLimit 70` gives text containing `6800` and `63`. With `problem: "x"` the title contains `x`. Mode fans-only (`limit 0, fanLimit 75`, not hot) starts with `thermal.cpu` and contains `75`. Off (`0, 0`) gives `thermal.offTitle`.
  - `toasts`:
    - fans on gives `thermal.fansOn`;
    - fans on plus slowing on in one step gives `thermal.slowingFans`;
    - fans off with `fanLimit > 0` gives `thermal.fansOff`;
    - fans off because `fanLimit` became 0 (mode changed) gives `null`;
    - both off gives `thermal.cooledFans`;
    - a new problem gives `thermal.fansProblem`, and the same problem again gives `null`;
    - today's slowing-only cases still give `thermal.slowing` / `thermal.cooled`.
  - `modeOptions`: `{unsupported: "u", ..}` disables `fansFirst`, `together` and `fans` with `why: "u"`, while `off` and `slow` stay enabled. `null` gives `why: t("fans.unknown")`. Supported gives every `why` null.
  - `clampFanLimit`: `(80,75) → 75`, `(10,75) → 50`, `(NaN,75) → 70`.

- [ ] **Step 2: Run them, expect failures**

Run: `pnpm vitest run src/thermal.test.ts`
Expected: failures and type errors.

- [ ] **Step 3: Implement.**
  - The title is the slowing title as today (or `thermal.cpu` when `limit == 0` and `fanLimit > 0`, or `thermal.offTitle` when both are 0), plus, when `fanLimit > 0`, a newline and one fan line: problem, forced or ready. `cool = fanLimit - COOL_MARGIN`.
  - `thermalToast` replaces the inline logic in `app.ts` `onThermal`. Precedence: a combined toast first, then a single one, then a problem.
  - Add the copy table to both locale files.
  - In `app.ts`, add only the boot defaults `thermalMode: boot.layout.thermalMode ?? "slow"` and `fanLimit: boot.layout.fanLimit ?? DEFAULT_FAN_LIMIT` next to `thermalLimit`.

- [ ] **Step 4: Run the tests, expect them to pass**

Run: `pnpm typecheck && pnpm vitest run src/thermal.test.ts`
Expected: all pass.

- [ ] **Step 5: Commit** with the subject `The temperature chip shows the fans: ≋, rpm in its title and its own toasts`.

### Task 7: Options and app wiring

**Files:**
- Modify: `src/settings.ts` (Temperatura section in the `power` group, ctx), `src/app.ts` (ctx implementation, boot defaults, `onThermal`)

**Interfaces:**
- Consumes: everything from Task 6.
- Produces (ctx in `settings.ts`):
  - `setThermal(change: Partial<Pick<Layout, "thermalMode" | "thermalLimit" | "fanLimit">>): void` replaces `setThermalLimit`.
  - `fans(): FanInfo | null`, `installFans(): Promise<boolean>`, `uninstallFans(): Promise<void>`.

- [ ] **Step 1: Replace the checkbox** in the Temperatura section with these rows, each with `data-row`:
  - a `seg` row `settings.thermalMode` with the 5 `modeOptions`. A disabled button gets `disabled` and `title=why`. If five buttons do not fit the Options width, let the `seg` wrap (`flex-wrap: wrap`) rather than switching to a select.
  - `settings.thermalLimit` input (hidden in mode `off`);
  - `settings.fanLimit` input (only in `fansFirst`, clamped with `clampFanLimit`);
  - on a supported Mac only, the consent line: `fansInstalled` with a `fansRemove` button, `fansOutdated` with a `fansUpdate` button, or `fansAsk`;
  - the about text: `thermalAboutOff` for `off`; otherwise `thermalAbout` when the mode slows plus `thermalAboutFans` when it uses fans.

- [ ] **Step 2: Bind it.**
  - Clicking a fan mode while `consent !== "installed"` first awaits `ctx.installFans()`. On `false`, it re-renders unchanged; the previous mode stays.
  - `fansUpdate` calls `installFans()`.
  - `fansRemove` awaits `uninstallFans()`.
  - The limit inputs call `setThermal` on `change`.

- [ ] **Step 3: Wire `app.ts`** (the boot defaults are already in from Task 6).
  - Load `this.fanInfo = await api.fansInfo().catch(() => null)` at boot.
  - `installFans` stores the returned info and resolves `true`. On error it shows the toast `fans.error` and resolves `false`.
  - `uninstallFans` stores the info. Then a mode of `fansFirst` or `together` becomes `"slow"`, and `fans` becomes `"off"`, both via `setThermal`.
  - `onThermal` keeps the chip-only redraw and toasts `thermalToast(prev, s)` when it is not null. `setThermal` saves the layout and refreshes the chip, as `setThermalLimit` does today.

- [ ] **Step 4: Verify**

Run: `pnpm check`
Expected: passes. Then run `pnpm tauri dev`, open Options → Zasilanie, and check:
- five modes are shown;
- the fan threshold appears only in "Wiatraki, potem zwalnianie";
- the Options search for "wiatraki" finds the rows;
- picking a fan mode opens the macOS password dialog;
- **Cancel** leaves the previous mode selected and shows a toast.

- [ ] **Step 5: Commit** with the subject `Options choose what desk does when hot, and ask for the fan permission once`.

### Task 8: Docs and the hardware check

**Files:**
- Modify: `README.md` (the Temperatura row in "Wszystko inne"), `CHANGELOG.md` (a new top entry in Polish, in the style of the existing ones)

- [ ] **Step 1: Docs.**
  - README Temperatura row: add the five-mode choice, fans at full speed on macOS (Apple Silicon) after a one-time admin password, back to auto 7°C lower, and "desk does not touch fans another program controls".
  - CHANGELOG: one bold-led bullet in the same voice.

- [ ] **Step 2: Hardware check on the M3 Pro.** The user does this, because it needs the admin password. Use a dev or local release build with the helper present.
  1. Options → Zasilanie → "Wiatraki, potem zwalnianie", set the fan threshold to 50 so it triggers without load. The password dialog appears once. Then `ls -l /Library/PrivilegedHelperTools/pl.makarowski.desk.fans /etc/sudoers.d/desk-fans` shows `root wheel`, `-rwxr-xr-x` and `-r--r-----`.
  2. Within ~20 s the chip shows `≋`, and `/Applications/Stats.app/Contents/Resources/smc fans` shows both fans near 6800 rpm in a forced mode.
  3. Set the threshold back to 70 (the CPU is cooler). Within ~60–75 s the chip loses `≋` and `smc fans` shows auto.
  4. Force again (threshold 50), then `kill -9` desk. Within ~5 s `smc fans` shows auto, and `~/Library/Application Support/desk/fans-forced` is gone.
  5. Force again and quit desk normally (⏻). The fans go back to auto at once.
  6. In Stats, set the fans to manual, then let desk get hot (threshold 50). desk does not touch them, and the chip title says "inny program". Set Stats back to auto.
  7. "Usuń zgodę" asks for the password, and both files disappear. The mode falls back to "Zwalniaj sesje".
  8. If step 2 fails because the M3 rejects the target before manual mode, swap the order in `force_max`, keeping the restore-on-error, update `force_max_sets_target_before_manual_mode_on_every_fan`, and repeat.

- [ ] **Step 3: Commit** with the subject `The README and changelog describe the fans when hot`.
