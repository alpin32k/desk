# Usage Details Popover Implementation Plan

**Goal:** A click on a usage chip in the top bar opens a popover with limits, forecast, model limits (Fable), resets, the sessions' share of the 5 h window and history; Options get an "AI stats" group to pick the popover layout and how the share is computed.

**Architecture:** Rust reads more of what desk already has: the cached `/api/oauth/usage` answer (model limits, severity, credits, resets), the `.status` files (per-session model, cost, cache), a new append-only history file, and (share mode A) Claude Code transcripts with a price table. The `usage` command (every 30 s) gains the light fields and records history; a new `usage_details` command, called only while the popover is open, returns history, sessions and the share. The frontend computes forecasts and chart buckets in pure functions and renders the popover outside `.top`.

**Tech Stack:** Tauri 2, Rust (serde_json, tempfile in tests), TypeScript, vitest, plain DOM + CSS.

**Spec:** `docs/superpowers/specs/2026-11-14-usage-details-design.md` (Polish). Read it before any task; copy (UI texts) and visual rules come from it.

## Global Constraints

- Branch `feat/usage-details`. Commit subject in English (`feat(usage): …`), body in Polish. No AI attribution anywhere.
- Code and comments in English; UI texts only through `src/locales/pl.ts` and `en.ts` (Polish copy from the spec, English translated).
- No new requests to Anthropic: every new field comes from the answer `usage::account` already fetches every 5 min. Token never logged or written.
- Transcripts read-only. Transcript scan never on the UI thread (`spawn_blocking`).
- History file `<support_dir>/usage-history.jsonl`, kept 8 days (`8 * 86_400` s).
- Defaults: `usageLayout` `"tabs"`, `usageShare` `"cost"`; unknown values fall back to them.
- Prices: the spec's table, `PRICED_ON = "2026-11-14"`, source `https://platform.claude.com/docs/en/about-claude/pricing`.
- Old `layout.json` without the new keys loads unchanged.
- Done means `pnpm check` passes (typecheck, eslint, vitest, `cargo clippy --all-targets -- -D warnings`, `cargo test`).

## Review Focus

1. **No API answer** (offline, expired login, 401): chip and popover still work from `.status` data; model limits, credits and resets sections simply absent. Test in Task 1 (`claude_without_api_answer_has_no_models`) and Task 9 (`renderNow` without models has no model section).
2. **A window that has already reset** (`resets_at` in the past, stale data): no forecast, no negative times. Test in Task 7 (`forecast` returns null when `resetsAt <= now`).
3. **First transcript scan is large** (hundreds of MB): the popover opens at once, the sessions tab shows a loading line until the share arrives; a second request while one runs reuses it. Test in Task 6 (`scan_is_incremental`: second scan reads only appended bytes).
4. **Several Claude accounts:** a popover shows only its account's sessions, share and history. Tests in Task 3 (`sessions_belong_to_their_account`) and Task 5 (`share_counts_only_the_account`).
5. **The chip disappears while open** (last Claude tile closed, account removed) or the window loses focus: the popover closes, nothing is left floating. Test in Task 10 (`closes_when_its_chip_is_gone`).

---

### Task 1: Model limits, severity, credits and resets from the API answer

**Files:**
- Modify: `src-tauri/src/usage.rs`
- Test: `src-tauri/src/usage.rs` (`mod tests`)

**Interfaces:**
- Produces (Rust, all `#[derive(Debug, Clone, PartialEq, Serialize)] #[serde(rename_all = "camelCase")]`):
  - `Window { label: String, used_percent: f64, resets_at: u64, severity: Option<String> }` (new field; `window()` sets `None`)
  - `ModelLimit { name: String, used_percent: f64, resets_at: u64, severity: Option<String>, share_of_weekly: Option<f64>, allowance_of_weekly: Option<f64> }`
  - `Resets { five_hour: Option<FiveHourReset>, saved: Vec<SavedReset> }`
  - `FiveHourReset { available: bool, next_available_at: Option<u64> }`
  - `SavedReset { label: String, left: u32, total: u32, ends_at: Option<u64>, requires_limit: bool }`
  - `Usage { agent, account, windows, models: Vec<ModelLimit>, credits: Option<bool>, resets: Option<Resets> }`
  - `pub fn account_windows(v: &Value, now: u64) -> Option<Vec<Window>>` (now fills `severity` from `limits[]`: `kind` `"session"` → 5h, `"weekly_all"` → 7d)
  - `pub fn account_models(v: &Value, now: u64) -> Vec<ModelLimit>` (`limits[]` with `kind == "weekly_scoped"`; name from `scope.model.display_name`, else `scope.model.id`; shares from `weekly_scoped_shares[].limit_index` == index in `limits`; expired dropped)
  - `pub fn account_credits(v: &Value) -> Option<bool>` (`extra_usage.is_enabled || spend.enabled`; `None` when both absent)
  - `pub fn account_resets(v: &Value, now: u64) -> Option<Resets>` (`juniper_tide` when `eligible`; `cedar_ember.grants` with `resets_left > 0` and not `paused`; `None` when nothing)
  - `pub fn status_models(status: &Value, now: u64) -> Vec<ModelLimit>` (`rate_limits.model_scoped[]`: `display_name`, `utilization`, `resets_at` ISO)
  - `fn account_answer(dir: &str, home: &Path, now: u64) -> Option<Value>`: today's `account()` cache, returning the raw answer

- [ ] **Step 1: Write the failing tests.** Put the real answer from the spec ("Co mówi Anthropic") in a test constant `ANSWER_2026_11_14` with all three `limits`, `weekly_scoped_shares`, `extra_usage.is_enabled: false`, `spend.enabled: false`, `juniper_tide: null`, `cedar_ember: null`.

```rust
#[test]
fn reads_model_limits_severity_and_credits() {
    let v: Value = serde_json::from_str(ANSWER_2026_11_14).unwrap();
    let now = iso_seconds("2026-11-14T01:00:00Z").unwrap();
    let w = account_windows(&v, now).unwrap();
    assert_eq!(w[0].severity.as_deref(), Some("normal"));
    assert_eq!(account_models(&v, now), vec![ModelLimit {
        name: "Fable".into(), used_percent: 95.0,
        resets_at: iso_seconds("2026-11-20T03:59:59.598916+00:00").unwrap(),
        severity: Some("critical".into()), share_of_weekly: Some(48.0), allowance_of_weekly: Some(50.0),
    }]);
    assert_eq!(account_credits(&v), Some(false));
    assert_eq!(account_resets(&v, now), None);
}

#[test]
fn reads_resets_when_offered() {
    let v = json!({
        "juniper_tide": {"eligible": true, "available": false, "next_available_at": "2026-11-20T04:00:00Z"},
        "cedar_ember": {"eligible": true, "grants": [
            {"id": "g1", "label": "Reset", "resets_total": 2, "resets_left": 1, "ends_at": "2026-11-27T00:00:00Z",
             "paused": false, "use_requires_limit": true, "clears": ["five_hour"]},
            {"id": "g2", "label": "Used up", "resets_total": 1, "resets_left": 0, "paused": false, "use_requires_limit": true}
        ]}
    });
    let r = account_resets(&v, 0).unwrap();
    assert_eq!(r.five_hour, Some(FiveHourReset { available: false, next_available_at: iso_seconds("2026-11-20T04:00:00Z") }));
    assert_eq!(r.saved, vec![SavedReset { label: "Reset".into(), left: 1, total: 2,
        ends_at: iso_seconds("2026-11-27T00:00:00Z"), requires_limit: true }]);
    assert_eq!(account_resets(&json!({"juniper_tide": {"eligible": false}}), 0), None);
}

#[test]
fn model_limits_from_the_status_line() {
    let s = json!({"rate_limits": {"model_scoped": [
        {"display_name": "Fable", "utilization": 80.0, "resets_at": "2026-11-20T03:59:59Z"}]}});
    assert_eq!(status_models(&s, 0)[0].used_percent, 80.0);
    assert!(status_models(&json!({}), 0).is_empty());
}

```

  And `claude_without_api_answer_has_no_models`: the setup of `reads_newest_codex_rollout_and_status_files` (temp home, no login, so no answer); assert `u[0].models.is_empty()`, `u[0].credits == None`, `u[0].resets == None`, and `u[0].windows[0].severity == None`.

- [ ] **Step 2: Run** `cd src-tauri && cargo test usage::` — expected: compile errors (types missing).
- [ ] **Step 3: Implement** the types and functions above; `claude()` builds each account's `Usage` from `account_answer` (windows via `freshest` as today, models from the answer, else freshest `status_models` of its sessions: per name the later `resets_at`, same reset the higher %), `credits`, `resets`. Codex gets empty `models`, `None`, `None`. Update the existing `Window { .. }` literals in tests with `severity: None`.
- [ ] **Step 4: Run** `cd src-tauri && cargo test usage::` — expected: all pass.
- [ ] **Step 5: Commit** `feat(usage): read model limits, severity, credits and resets`.

### Task 2: Options group "AI stats" and the two layout keys

**Files:**
- Modify: `src-tauri/src/layout.rs`, `src/api.ts` (`Layout`), `src/app.ts` (bootstrap defaults ~line 340, settings context ~line 200), `src/settings.ts`, `src/locales/pl.ts`, `src/locales/en.ts`
- Test: `src-tauri/src/layout.rs` (`mod tests`), `src/settings-search.test.ts` (or a new `src/settings-stats.test.ts` if search tests don't render panes)

**Interfaces:**
- Produces: `Layout.usageLayout: "tabs" | "all"`, `Layout.usageShare: "cost" | "transcripts"` (TS); `usage_layout: String`, `usage_share: String` (Rust, defaults `"tabs"`, `"cost"`); `SettingsContext.setUsageLayout(v: "tabs" | "all")`, `SettingsContext.setUsageShare(v: "cost" | "transcripts")`; `Group` gains `"stats"` placed after `"claude"`.

- [ ] **Step 1: Failing test** in `layout.rs`:

```rust
#[test]
fn usage_options_default_unless_the_file_says_otherwise() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("layout.json");
    std::fs::write(&p, r#"{"version":1}"#).unwrap();
    assert_eq!((load(&p).usage_layout.as_str(), load(&p).usage_share.as_str()), ("tabs", "cost"));
    std::fs::write(&p, r#"{"version":1,"usageLayout":"all","usageShare":"transcripts"}"#).unwrap();
    assert_eq!((load(&p).usage_layout.as_str(), load(&p).usage_share.as_str()), ("all", "transcripts"));
}
```

- [ ] **Step 2: Run** `cd src-tauri && cargo test layout::` — expected: FAIL (no such field).
- [ ] **Step 3: Implement.** Rust fields + defaults. TS `Layout` fields, bootstrap `usageLayout: boot.layout.usageLayout === "all" ? "all" : "tabs"`, same for share. Settings pane `"stats"` with two `seg` rows exactly like "Klik w link otwiera" (`data-usage-layout`, `data-usage-share`), the share row's hint under it changes with the choice. Keys and Polish copy: `settings.stats` „Statystyki AI”, `settings.usageLayout` „Szczegóły limitów”, `settings.usageLayoutTabs` „Zakładki”, `settings.usageLayoutAll` „Wszystko naraz”, `settings.usageShare` „Udział sesji w oknie 5 h”, `settings.usageShareCost` „Z kosztu sesji desk”, `settings.usageShareTranscripts` „Z transkryptów Claude Code”, `settings.usageShareHintCost` „Tylko sesje desk; pierwsze okno po starcie desk niepełne.”, `settings.usageShareHintTranscripts` „Wszystkie sesje Claude Code na tym komputerze; ceny z 14.11.2026.”
- [ ] **Step 4: Settings search test:** typing „udział” and „transkrypt” finds the share row, „limit” finds the layout row (follow the existing search test's setup).
- [ ] **Step 5: Run** `cd src-tauri && cargo test layout:: && cd .. && pnpm test` — expected: pass.
- [ ] **Step 6: Commit** `feat(settings): add the AI stats group`.

### Task 3: Session statuses

**Files:**
- Create: `src-tauri/src/usage_sessions.rs` (declare `pub mod usage_sessions;` in `lib.rs`)
- Test: same file

**Interfaces:**
- Produces:
  - `#[derive(Debug, Clone, PartialEq, Serialize)] #[serde(rename_all = "camelCase")] pub struct SessionStatus { slug: String, session_id: String, transcript: String, account: String, model: String, model_id: String, effort: Option<String>, fast: bool, cost: f64, context_percent: Option<f64>, cache_warm: Option<bool>, cache_expires_at: Option<u64>, recache_tokens: Option<u64> }`
  - `pub fn read(state_dir: &Path, accounts: &[ClaudeAccount], home: &Path) -> Vec<SessionStatus>`: every `<slug>.status` with a `session_id`; `account` = `accounts::owner_of(effective accounts, transcript)` id, files with no owner skipped; `model` = `model.display_name`, else `model.id`.

- [ ] **Step 1: Failing tests:** `reads_a_status_file` (a status JSON with the fields from the spec's "Ograniczenia z kodu" → exact struct, e.g. cost `0.44`, `context_percent` `Some(7.0)`, `cache_expires_at` `Some(1794621281)`, `effort` `Some("xhigh")`, slug from the file name `desk-dropdown-fcb86df5`); `skips_files_without_a_session`; `sessions_belong_to_their_account` (two accounts as in `usage.rs::each_account_gets_the_limits_of_its_own_sessions` → each status gets its account id).
- [ ] **Step 2: Run** `cd src-tauri && cargo test usage_sessions::` — FAIL.
- [ ] **Step 3: Implement** `read`.
- [ ] **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `feat(usage): read per-session status for the popover`.

### Task 4: History recording

**Files:**
- Create: `src-tauri/src/usage_history.rs` (`pub mod usage_history;`)
- Modify: `src-tauri/src/lib.rs` (`App` gets `history: Arc<Mutex<usage_history::Recorder>>`; the `usage` command records after computing)
- Test: `usage_history.rs`

**Interfaces:**
- Produces:
  - `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]` `LimitSample { t: u64, agent: String, account: Option<String>, label: String, pct: f64, resets: u64 }` and `CostSample { t: u64, session: String, slug: String, account: Option<String>, cost: f64 }` (field names exactly as in the spec's JSON lines), `#[serde(untagged)] enum Line { Limit(LimitSample), Cost(CostSample) }`
  - `pub const KEEP_SECS: u64 = 8 * 86_400;`
  - `pub struct Recorder` with `pub fn open(path: PathBuf) -> Recorder` (seeds the last value per key from the file), `pub fn record(&mut self, now: u64, limits: &[LimitSample], costs: &[CostSample]) -> std::io::Result<()>` (appends only changed values: limit key `(agent, account, label)` compares `pct` and `resets`, cost key `session` compares `cost`; prunes on the first call and then once per 86 400 s), `pub fn load(&self, since: u64) -> (Vec<LimitSample>, Vec<CostSample>)` (skips unreadable lines), `pub fn since(&self) -> Option<u64>` (time of the oldest kept line)
  - `pub fn limit_samples(now: u64, usages: &[usage::Usage]) -> Vec<LimitSample>` (windows and model limits; model label = its name)

- [ ] **Step 1: Failing tests:** `records_only_changes` (same samples twice → 1 line; pct changes → 2), `keeps_eight_days` (line at `now - KEEP_SECS - 1` gone after prune, `now - 10` kept), `skips_a_broken_line` (garbage line between two good ones → both loaded), `reopen_does_not_repeat` (new `Recorder::open` on the same file, same sample → no new line).
- [ ] **Step 2: Run** `cd src-tauri && cargo test usage_history::` — FAIL.
- [ ] **Step 3: Implement**; wire into `usage` command: after `usage::usage(..)`, `recorder.record(now, &limit_samples(now, &u), &cost_samples)` where cost samples come from `usage_sessions::read` (Claude only). Errors are logged once and ignored (history must never break the chip).
- [ ] **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `feat(usage): record limit and cost history`.

### Task 5: Share mode B (from session cost)

**Files:**
- Create: `src-tauri/src/usage_share.rs` (`pub mod usage_share;`)
- Test: same file

**Interfaces:**
- Produces:
  - `#[derive(Debug, Clone, PartialEq, Serialize)] #[serde(rename_all = "camelCase")] pub struct ShareRow { slug: Option<String>, label: String, fraction: f64 }` (`slug` None = not a desk session; `label` = slug or project name)
  - `pub struct Share { mode: String /* "cost" | "transcripts" */, rows: Vec<ShareRow>, incomplete: bool }` (same derives)
  - `pub fn from_costs(costs: &[CostSample], account: Option<&str>, window_start: u64, recording_since: Option<u64>) -> Share` (rule from the spec: baseline = last sample ≤ `window_start`; none: 0 when `recording_since <= window_start`, else the first sample in the window and `incomplete = true`; negative deltas clamp to 0; zero deltas dropped; fractions sum to 1; rows sorted by fraction desc)

- [ ] **Step 1: Failing tests** (window_start = 1000):

```rust
#[test]
fn share_from_cost_deltas() {
    let c = |t, s: &str, cost| CostSample { t, session: s.into(), slug: s.into(), account: None, cost };
    let s = from_costs(&[c(900, "a", 2.0), c(1500, "a", 5.0), c(1200, "b", 0.5), c(1600, "b", 1.5)], None, 1000, Some(800));
    assert!(!s.incomplete);
    assert_eq!(s.rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    assert!((s.rows[0].fraction - 3.0 / 4.5).abs() < 1e-9);
    let s = from_costs(&[c(1150, "a", 2.0), c(1500, "a", 5.0)], None, 1000, Some(1100));
    assert!(s.incomplete);
    assert_eq!(s.rows[0].fraction, 1.0);
}
```

  And `share_counts_only_the_account`: samples of session `w` (account `Some("work")`, 1.0 → 3.0) and `o` (account `Some("own")`, 1.0 → 9.0) around the window start; `from_costs(.., Some("work"), ..)` has exactly one row, `w`, with fraction 1.0.

- [ ] **Step 2: Run** `cargo test usage_share::` — FAIL. **Step 3: Implement.** **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `feat(usage): share of the 5h window from session cost`.

### Task 6: Prices and share mode A (from transcripts)

**Files:**
- Create: `src-tauri/src/pricing.rs`, `src-tauri/src/usage_transcripts.rs` (both `pub mod`)
- Modify: `src-tauri/src/usage_share.rs` (`from_transcripts`)
- Test: in each file

**Interfaces:**
- Produces:
  - `pub struct Price { input: f64, write_5m: f64, write_1h: f64, read: f64, output: f64 }` ($/MTok); `pub const PRICED_ON: &str = "2026-11-14";`
  - `pub fn price(model: &str, prompt_tokens: u64, fast: bool) -> Price`: the spec's table keyed by model id (`claude-fable-5-2`, `claude-mythos-5-2`, …, `claude-haiku-4-5`); Haiku 5.5 uses its high tier when `prompt_tokens > 100_000`; `fast` uses the fast input/output prices and scales the cache prices by the same factor as input (decision: the pricing page gives only input/output for fast mode); unknown id → newest row of its family (`fable`/`mythos`, `opus`, `sonnet`, `haiku` in the id), no family → Opus 5.5.
  - `pub struct Tokens { input: u64, write_5m: u64, write_1h: u64, read: u64, output: u64 }`; `pub fn cost(model: &str, t: &Tokens, fast: bool) -> f64` (prompt tokens for the Haiku tier = input + writes + read)
  - `pub struct Scanner` with `pub fn scan(&mut self, projects_dir: &Path, window_start: u64) -> HashMap<String /* parent transcript path */, (f64 /* cost */, String /* cwd basename */)>`: files under `projects_dir` modified since `window_start`; per file remembers the byte offset, the seen `(message.id, requestId)` pairs and per-message `(timestamp, cost)`; a file shorter than its offset is read from 0; `<proj>/<session>/subagents/*.jsonl` counts to `<proj>/<session>.jsonl`; only `type == "assistant"` with `timestamp >= window_start`; cache writes from `usage.cache_creation.ephemeral_5m_input_tokens` / `ephemeral_1h_input_tokens`, else `cache_creation_input_tokens` as 5m; `usage.speed == "fast"` → fast prices.
  - `usage_share::from_transcripts(costs: &HashMap<String, (f64, String)>, sessions: &[SessionStatus], account: &str) -> Share`: transcript path → session slug (rows with `slug`), others one row per cwd basename (`slug: None`); mode `"transcripts"`.

- [ ] **Step 1: Failing tests** in `pricing.rs`: `cost("claude-fable-5-2", input 1_000_000)` = 8.0; output 1M on `claude-opus-5-5` = 20.0; read 1M on Fable 5.2 = 0.20; write_1h 1M on Opus 5.5 = 8.0; Haiku 5.5 input 200_000 → 0.10 (0.50 $/MTok tier); fast Opus 5.5 input 1M = 8.0; `claude-fable-9` priced as Fable 5.2; `"gpt-x"` priced as Opus 5.5.
- [ ] **Step 2: Failing tests** in `usage_transcripts.rs` (temp `projects/-p/`): `counts_a_message_once` (same id+requestId 3×), `ignores_before_the_window`, `subagents_count_to_their_session`, `splits_cache_writes`, `scan_is_incremental` (append a line, second scan returns old + new cost and reads from the stored offset), `rereads_a_truncated_file`.
- [ ] **Step 3: Run** `cargo test pricing:: usage_transcripts::` — FAIL. **Step 4: Implement.** **Step 5: Run** — PASS.
- [ ] **Step 6: Commit** `feat(usage): share of the 5h window from transcripts`.

### Task 7: The `usage_details` command and the frontend data helpers

**Files:**
- Modify: `src-tauri/src/lib.rs` (command + `App` holds `scanner: Arc<Mutex<usage_transcripts::Scanner>>` and a 30 s result cache), `src/api.ts`
- Create: `src/usage-stats.ts`, `src/usage-stats.test.ts`

**Interfaces:**
- Produces (Rust): `UsageDetails { history: Vec<LimitSample>, recording_since: Option<u64>, sessions: Vec<SessionStatus>, share: Option<Share> }` (camelCase); `#[tauri::command(async)] usage_details(agent: String, account: Option<String>, share: Option<String>) -> Result<UsageDetails, String>`; `share` `None` skips the share (the tabs layout asks for it only on the sessions tab), `"transcripts"` runs mode A, anything else mode B. History filtered to the agent and account. Window start = the latest 5h `LimitSample.resets - 18_000` of that account (none → `share: None`). Codex: empty `sessions`, `share: None`. Register in `generate_handler!`.
- Produces (TS, `api.ts`): interfaces mirroring every Rust type above (`UsageWindow.severity: string | null`, `ModelLimit`, `Resets`, `FiveHourReset`, `SavedReset`, `Usage.models/credits/resets`, `LimitSample`, `SessionStatus`, `ShareRow`, `Share`, `UsageDetails`); `api.usageDetails(agent: string, account: string | null, share: "cost" | "transcripts" | null): Promise<UsageDetails>`.
- Produces (TS, `usage-stats.ts`):
  - `windowSeconds(label: string): number | null` ("5h" → 18000, "7d" → 604800, "Nh"/"Nd" generally, model names → 604800 when given `weekly: true`)
  - `interface Forecast { pacePerHour: number; endsAt: number | null; beforeReset: number; endPercent: number; dailyBudget: number | null }` (`endsAt`/`beforeReset` in Unix s / s; `beforeReset > 0` only when 100% comes before the reset; `dailyBudget` only for windows of 2 days or more)
  - `forecast(usedPercent: number, resetsAt: number, lengthSec: number, nowMs: number): Forecast | null` (null when elapsed < 10% of the window or `resetsAt <= now`)
  - `severityLevel(severity: string | null, percent: number): "" | "warn" | "high"` (`critical` → high, `warning` → warn, `normal` → "", null → `level(percent)`)
  - `chipState(u: Usage, nowMs: number): { level: "" | "warn" | "high"; dot: boolean; reasons: string[] }`
  - `bars24h(history: LimitSample[], nowMs: number): { percent: number; level: "" | "warn" | "high" }[]` (48 slots of 30 min ending now, max 5h `pct` per slot, carried forward inside a window, 0 after its reset)
  - `resetMarks(history: LimitSample[], nowMs: number): { at: number; x: number }[]` (5h resets inside the last 24 h, `x` 0..1)
  - `fullSpans(history: LimitSample[], nowMs: number): { from: number; to: number }[]` (5h at ≥ 100)
  - `weekDays(history: LimitSample[], label: string, resetsAt: number): { dayStart: number; delta: number }[]` (local days from `resetsAt - 604800` to `resetsAt`; delta = growth of that label's max pct within the day)

- [ ] **Step 1: Failing tests** in `usage-stats.test.ts` (now = `Date.UTC(2026, 10, 14, 0, 55)`; values from the mockups):

```ts
const now = Date.UTC(2026, 10, 14, 0, 55);
const s = now / 1000;
it("forecasts the 5h window running out before its reset", () => {
  const f = forecast(50, s + 2 * 3600 + 55 * 60, 18000, now)!;
  expect(f.pacePerHour).toBeCloseTo(24, 1);
  expect(f.beforeReset).toBeCloseTo(50 * 60, -1);
});
it("gives a daily budget for the week", () => {
  const f = forecast(52, s + (6 * 24 + 3) * 3600 + 5 * 60, 604800, now)!;
  expect(f.dailyBudget).toBeCloseTo(7.83, 1);
  expect(f.endsAt! - s).toBeCloseTo(19.3 * 3600, -3);
});
it("has no forecast too early or after the reset", () => {
  expect(forecast(5, s + 4 * 3600 + 40 * 60, 18000, now)).toBeNull();
  expect(forecast(50, s - 1, 18000, now)).toBeNull();
});
it("turns the chip red with a dot for Fable at 95% critical", () => {
  const u = { agent: "claude", account: null, credits: false, resets: null,
    windows: [{ label: "5h", usedPercent: 61, resetsAt: s + 6840, severity: "normal" },
              { label: "7d", usedPercent: 54, resetsAt: s + 5 * 86400, severity: "normal" }],
    models: [{ name: "Fable", usedPercent: 95, resetsAt: s + 5 * 86400, severity: "critical", shareOfWeekly: 48, allowanceOfWeekly: 50 }] };
  const c = chipState(u, now);
  expect([c.level, c.dot]).toEqual(["high", true]);
  expect(c.reasons.join(" ")).toContain("Fable 95%");
});
it("keeps today's chip without model limits or warnings", () => {
  const u = { agent: "claude", account: null, credits: null, resets: null, models: [],
    windows: [{ label: "5h", usedPercent: 20, resetsAt: s + 3600, severity: null }] };
  expect(chipState(u, now)).toMatchObject({ level: "", dot: false });
});
```

  plus `bars24h` (samples 40 → 60 → reset → 10 give the right slots and levels), `resetMarks` (one reset 12 h ago → `x` ≈ 0.5), `weekDays` (47 on day 1, 52 on day 2 → deltas 47 and 5).
- [ ] **Step 2: Run** `pnpm test usage-stats` — FAIL. **Step 3: Implement** both sides. **Step 4: Run** `pnpm test && cd src-tauri && cargo test` — PASS.
- [ ] **Step 5: Commit** `feat(usage): details command and forecast helpers`.

### Task 8: The chip as a button, colored by every limit

**Files:**
- Modify: `src/usage.ts`, `src/usage.test.ts`, `src/styles.css`, locales

**Interfaces:**
- Consumes: `chipState` (Task 7).
- Produces: `chipId(u: Usage): string` = `` `${u.agent}:${u.account ?? ""}` ``; `usageHtml` renders `<button class="usage {level}" data-usage="{chipId}" aria-haspopup="dialog" title="…">…{dot ? '<i class="dot"></i>' : ""}</button>`; title = today's lines plus `chipState.reasons` (each reason a locale string, e.g. `usage.reasonModel` „{name} {percent}% (krytyczny)”, `usage.reasonForecast` „{label}: przy tym tempie koniec {when}”).

- [ ] **Step 1: Update tests:** the existing expectation now matches `<button class="usage high" data-usage="claude:" …` and the inner text stays `claude 5h 42% <span class="reset">↻2h00m</span> · 7d 91% …`; new test: the Fable example renders `class="usage high"` and `<i class="dot"></i>`.
- [ ] **Step 2: Run** `pnpm test usage` — FAIL. **Step 3: Implement**; CSS: `.usage` as button keeps today's look (font, border, padding; no button background), `.usage .dot` 6 px circle in `currentColor`, `.usage[aria-expanded="true"]` border `--accent`.
- [ ] **Step 4: Run** — PASS. **Step 5: Commit** `feat(usage): color the chip by every limit`.

### Task 9: Popover content (HTML builders)

**Files:**
- Create: `src/usage-view.ts`, `src/usage-view.test.ts`
- Modify: `src/themes.ts` (`UiVar` gains `"model"` = `t.ansi[5]`), `src/styles.css`, locales

**Interfaces:**
- Consumes: Task 7 types and helpers.
- Produces:
  - `interface SessionView { slug: string; key: string; name: string; state: "working" | "waiting" | "idle"; status: SessionStatus }` (built by the app from tiles)
  - `renderNow(u: Usage, nowMs: number): string`: sections 5h, week, model limits, resets, exactly as the spec's "Teraz" and "Wygląd" say
  - `renderSessions(rows: SessionView[], share: Share | null | "loading", wide: boolean, nowMs: number): string` (`wide` adds the context column; rows sorted by share then cost; "reszta" row = fractions of share rows whose slug is not among `rows`; cold-cache line for the largest `recacheTokens` > 50 000; `share.incomplete` adds „niepełne” to the share header; rows carry `data-key`)
  - `renderHistory(d: UsageDetails, u: Usage, nowMs: number): string` (24 h bars with reset lines and the full-limit line; week days with the equal-share line at 100/7 % and a thinner model bar per day per model limit; empty state when `recordingSince` is within the last 24 h)
  - `renderPopover(opts: { u: Usage; details: UsageDetails | null; share: Share | null | "loading"; rows: SessionView[]; layout: "tabs" | "all"; tab: "now" | "sessions" | "history"; narrow: boolean; nowMs: number; title: string }): string` (tabs: tablist with `data-tab`, dot on "now" from `chipState`, session count; Codex has no sessions tab; all: two columns + sessions, one column when `narrow`)

- [ ] **Step 1: Failing tests:** `renderNow` with the Fable example contains „Fable”, „95%”, „kredyty wyłączone” and the class for critical; without models it has no model section; with `resets` it contains „/limit-reset” and „1 z 2”; `renderPopover` tabs has three tabs and the dot on "now", Codex has two; `renderSessions` with share `{a: 0.6, closed: 0.4}` and only `a` open shows a „reszta” row with 40%; `"loading"` shows the loading text.
- [ ] **Step 2: Run** `pnpm test usage-view` — FAIL. **Step 3: Implement**, copy from the spec into locales (`usage.*` keys), styles from "Wygląd". **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `feat(usage): popover views`.

### Task 10: Popover behavior and app wiring

**Files:**
- Create: `src/usage-popover.ts`, `src/usage-popover.test.ts`
- Modify: `src/app.ts`, `src/styles.css`

**Interfaces:**
- Consumes: `renderPopover` (Task 9), `api.usageDetails` (Task 7), `Layout.usageLayout/usageShare` (Task 2).
- Produces: `class UsagePopover { constructor(host: HTMLElement, ctx: PopoverContext); toggle(chipId: string): void; close(): void; refresh(): void; get openId(): string | null }` with `interface PopoverContext { usage(): Usage[]; layout(): Layout; sessions(agent: string, account: string | null): SessionView[]; details(agent: string, account: string | null, share: "cost" | "transcripts" | null): Promise<UsageDetails>; focus(key: string): void; title(u: Usage): string }`; pure helpers `stillOpen(openId: string | null, usage: Usage[]): string | null` (null when no `Usage` has that `chipId`) and `shareRequest(layout: Layout, tab: "now" | "sessions" | "history"): "cost" | "transcripts" | null` (tabs layout: only on `"sessions"`; all layout: always).

Behavior (from the spec): opens under the chip found by `[data-usage="<chipId>"]` in `.top`, right-aligned with a caret; toggle, Esc, click outside and window `blur` close; `refresh()` (called at the end of `renderChrome` and after `refreshUsage`) re-renders keeping tab and scroll, re-positions, and closes when the chip is gone; details fetched on open and on each refresh while open (one request at a time); last tab kept in memory; row click → `ctx.focus(key)` and close; `narrow` when `window.innerWidth < 960 + 32`; height capped to the window, inside scrolls; `aria-expanded` on the chip.

- [ ] **Step 1: Failing tests** (vitest runs without a DOM here; no DOM package is added): `closes_when_its_chip_is_gone`: `stillOpen("claude:work", [usage of "claude:own" only])` is `null`, with `"claude:work"` present it returns `"claude:work"`; `shareRequest`: tabs + `"now"` → `null`, tabs + `"sessions"` → layout's share, all + `"history"` → layout's share, `usageShare: "transcripts"` passes through. The DOM behavior is checked in Step 5.
- [ ] **Step 2: Run** `pnpm test usage-popover` — FAIL. **Step 3: Implement**; in `app.ts`: create the popover on `#app`, handle `[data-usage]` clicks in the top click listener, build `SessionView`s from `this.tiles` (Claude tiles of that account via `layout.tileAccounts`, slug, name, `displayState`) joined with `details.sessions` by slug, `focus` = `setActive(key, true)`, settings context setters from Task 2 call `this.save()` and `popover.refresh()`.
- [ ] **Step 4: Run** `pnpm check` — PASS.
- [ ] **Step 5: Manual check** with `pnpm tauri dev`: open/close each way, popover survives a refresh (wait 30 s), dropdown mode, a second account, a Codex tile, a light theme, both layouts and both share modes.
- [ ] **Step 6: Commit** `feat(usage): open the details popover from the chip`.

### Task 11: README

**Files:**
- Modify: `README.md` (Polish, next to the existing "Zero tokenów" / limits description)

- [ ] **Step 1:** Describe the popover in 4–6 lines: forecast, model limits, resets, sessions' share (two ways, Options → Statystyki AI), history kept 8 days in `usage-history.jsonl`.
- [ ] **Step 2: Run** `pnpm check` — PASS.
- [ ] **Step 3: Commit** `docs: describe the usage details popover`.
