//! History of the plan limits and of each session's cost, for the usage
//! popover's charts and the share of the 5-hour window (usage_share.rs).
//! One JSON line per change in `<support_dir>/usage-history.jsonl`, kept
//! for 8 days (and the last line of every session and limit, however old).

use crate::usage::Usage;
use crate::usage_sessions::SessionStatus;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub const KEEP_SECS: u64 = 8 * 86_400;
const PRUNE_EVERY: u64 = 86_400;

/// A limit's reading: a window ("5h", "7d") or a model limit ("Fable").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LimitSample {
    pub t: u64,
    pub agent: String,
    pub account: Option<String>,
    pub label: String,
    pub pct: f64,
    pub resets: u64,
}

/// A session's cost since it started (Claude Code's sum, at API prices).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostSample {
    pub t: u64,
    pub session: String,
    pub slug: String,
    pub account: Option<String>,
    pub cost: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Line {
    Limit(LimitSample),
    Cost(CostSample),
}

type LimitKey = (String, Option<String>, String);

pub struct Recorder {
    path: PathBuf,
    limits: HashMap<LimitKey, (f64, u64)>,
    costs: HashMap<String, f64>,
    pruned_at: Option<u64>,
}

impl Recorder {
    /// The file's last value of every limit and session, so a restart does
    /// not write them again.
    pub fn open(path: PathBuf) -> Recorder {
        let mut r = Recorder { path, limits: HashMap::new(), costs: HashMap::new(), pruned_at: None };
        let (limits, costs) = r.load(0);
        for l in limits {
            r.limits.insert(key(&l), (l.pct, l.resets));
        }
        for c in costs {
            r.costs.insert(c.session, c.cost);
        }
        r
    }

    /// Appends what changed since the last reading; drops lines older than
    /// 8 days on the first call and then once a day.
    pub fn record(&mut self, now: u64, limits: &[LimitSample], costs: &[CostSample]) -> std::io::Result<()> {
        if self.pruned_at.is_none_or(|at| now.saturating_sub(at) >= PRUNE_EVERY) {
            self.prune(now)?;
            self.pruned_at = Some(now);
        }
        let mut out = String::new();
        for l in limits {
            if self.limits.get(&key(l)) != Some(&(l.pct, l.resets)) {
                self.limits.insert(key(l), (l.pct, l.resets));
                out.push_str(&line(&Line::Limit(l.clone())));
            }
        }
        for c in costs {
            if self.costs.get(&c.session) != Some(&c.cost) {
                self.costs.insert(c.session.clone(), c.cost);
                out.push_str(&line(&Line::Cost(c.clone())));
            }
        }
        if out.is_empty() {
            return Ok(());
        }
        use std::io::Write;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?
            .write_all(out.as_bytes())
    }

    /// Readings at or after `since`; unreadable lines are skipped.
    pub fn load(&self, since: u64) -> (Vec<LimitSample>, Vec<CostSample>) {
        let text = std::fs::read_to_string(&self.path).unwrap_or_default();
        let mut limits = Vec::new();
        let mut costs = Vec::new();
        for l in text.lines().filter_map(|l| serde_json::from_str::<Line>(l).ok()) {
            match l {
                Line::Limit(x) if x.t >= since => limits.push(x),
                Line::Cost(x) if x.t >= since => costs.push(x),
                _ => {}
            }
        }
        (limits, costs)
    }

    /// When the oldest kept reading was taken.
    pub fn since(&self) -> Option<u64> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        text.lines()
            .filter_map(|l| serde_json::from_str::<Line>(l).ok())
            .map(|l| match l {
                Line::Limit(x) => x.t,
                Line::Cost(x) => x.t,
            })
            .min()
    }

    fn prune(&self, now: u64) -> std::io::Result<()> {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return Ok(());
        };
        let keep_from = now.saturating_sub(KEEP_SECS);
        let parsed: Vec<(&str, Line)> =
            text.lines().filter_map(|l| serde_json::from_str::<Line>(l).ok().map(|x| (l, x))).collect();
        // The last reading of every session and limit stays whatever its age:
        // it is where the next change is counted from (usage_share.rs).
        let id = |x: &Line| match x {
            Line::Limit(x) => format!("limit\0{}\0{}\0{}", x.agent, x.account.as_deref().unwrap_or(""), x.label),
            Line::Cost(x) => format!("cost\0{}", x.session),
        };
        let last: HashMap<String, usize> = parsed.iter().enumerate().map(|(i, (_, x))| (id(x), i)).collect();
        let kept: String = parsed
            .iter()
            .enumerate()
            .filter(|(i, (_, x))| {
                let t = match x {
                    Line::Limit(x) => x.t,
                    Line::Cost(x) => x.t,
                };
                t >= keep_from || last.get(&id(x)) == Some(i)
            })
            .map(|(_, (l, _))| format!("{l}\n"))
            .collect();
        if kept.len() == text.len() {
            return Ok(());
        }
        let tmp = self.path.with_extension("jsonl.tmp");
        std::fs::write(&tmp, kept)?;
        std::fs::rename(&tmp, &self.path)
    }
}

fn key(l: &LimitSample) -> LimitKey {
    (l.agent.clone(), l.account.clone(), l.label.clone())
}

fn line(l: &Line) -> String {
    serde_json::to_string(l).map(|s| s + "\n").unwrap_or_default()
}

/// Every window and model limit of these readings.
pub fn limit_samples(now: u64, usages: &[Usage]) -> Vec<LimitSample> {
    usages
        .iter()
        .flat_map(|u| {
            let sample = move |label: &str, pct: f64, resets: u64| LimitSample {
                t: now,
                agent: u.agent.clone(),
                account: u.account.clone(),
                label: label.to_string(),
                pct,
                resets,
            };
            u.windows
                .iter()
                .map(move |w| sample(&w.label, w.used_percent, w.resets_at))
                .chain(u.models.iter().map(move |m| sample(&m.name, m.used_percent, m.resets_at)))
        })
        .collect()
}

/// Each session's cost now; the account named only with two or more
/// accounts, as `Usage::account`.
pub fn cost_samples(now: u64, sessions: &[SessionStatus], split: bool) -> Vec<CostSample> {
    sessions
        .iter()
        .map(|s| CostSample {
            t: now,
            session: s.session_id.clone(),
            slug: s.slug.clone(),
            account: split.then(|| s.account.clone()),
            cost: s.cost,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limit(t: u64, pct: f64) -> LimitSample {
        LimitSample { t, agent: "claude".into(), account: None, label: "5h".into(), pct, resets: 5000 }
    }

    fn lines(r: &Recorder) -> usize {
        std::fs::read_to_string(&r.path).unwrap_or_default().lines().count()
    }

    #[test]
    fn records_only_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let mut r = Recorder::open(tmp.path().join("h.jsonl"));
        r.record(100, &[limit(100, 61.0)], &[]).unwrap();
        r.record(130, &[limit(130, 61.0)], &[]).unwrap();
        assert_eq!(lines(&r), 1);
        r.record(160, &[limit(160, 62.0)], &[]).unwrap();
        assert_eq!(lines(&r), 2);
        let c = |t, cost| CostSample { t, session: "s".into(), slug: "p-1".into(), account: None, cost };
        r.record(190, &[], &[c(190, 1.5)]).unwrap();
        r.record(220, &[], &[c(220, 1.5)]).unwrap();
        assert_eq!(lines(&r), 3);
        assert_eq!(r.load(0), (vec![limit(100, 61.0), limit(160, 62.0)], vec![c(190, 1.5)]));
        assert_eq!(r.since(), Some(100));
    }

    #[test]
    fn keeps_eight_days() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("h.jsonl");
        let now = 1_000_000;
        let old = serde_json::to_string(&limit(now - KEEP_SECS - 1, 10.0)).unwrap();
        let new = serde_json::to_string(&limit(now - 10, 20.0)).unwrap();
        std::fs::write(&path, format!("{old}\n{new}\n")).unwrap();
        let mut r = Recorder::open(path);
        r.record(now, &[], &[]).unwrap();
        assert_eq!(r.load(0).0, vec![limit(now - 10, 20.0)]);
    }

    #[test]
    fn keeps_the_last_reading_of_each_session_and_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("h.jsonl");
        let now = 1_000_000;
        let old = now - KEEP_SECS - 100;
        let c = |t, cost| CostSample { t, session: "s".into(), slug: "p-1".into(), account: None, cost };
        let lines = [
            serde_json::to_string(&c(old - 50, 1.0)).unwrap(),
            serde_json::to_string(&c(old, 2.0)).unwrap(),
            serde_json::to_string(&limit(old - 50, 10.0)).unwrap(),
            serde_json::to_string(&limit(old, 11.0)).unwrap(),
        ];
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let mut r = Recorder::open(path.clone());
        r.record(now, &[], &[]).unwrap();
        // Without its last reading a session idle for 8 days would count its
        // whole cost into the next window it is used in.
        assert_eq!(r.load(0), (vec![limit(old, 11.0)], vec![c(old, 2.0)]));
        let mut again = Recorder::open(path);
        again.record(now + 10, &[limit(now + 10, 11.0)], &[c(now + 10, 2.0)]).unwrap();
        assert_eq!(again.load(0).1.len(), 1, "a restart writes nothing new");
    }

    #[test]
    fn skips_a_broken_line() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("h.jsonl");
        let c = CostSample { t: 7, session: "s".into(), slug: "p-1".into(), account: Some("work".into()), cost: 2.0 };
        let a = serde_json::to_string(&limit(5, 1.0)).unwrap();
        let b = serde_json::to_string(&c).unwrap();
        std::fs::write(&path, format!("{a}\n{{broken\n{b}\n")).unwrap();
        assert_eq!(Recorder::open(path).load(0), (vec![limit(5, 1.0)], vec![c]));
    }

    #[test]
    fn reopen_does_not_repeat() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("h.jsonl");
        Recorder::open(path.clone()).record(100, &[limit(100, 61.0)], &[]).unwrap();
        let mut r = Recorder::open(path);
        r.record(200, &[limit(200, 61.0)], &[]).unwrap();
        assert_eq!(lines(&r), 1);
    }

    #[test]
    fn samples_from_usage_and_sessions() {
        use crate::usage::{ModelLimit, Window};
        let u = Usage {
            agent: "claude".into(),
            account: Some("work".into()),
            windows: vec![Window { label: "5h".into(), used_percent: 61.0, resets_at: 9000, severity: None }],
            models: vec![ModelLimit {
                name: "Fable".into(),
                used_percent: 95.0,
                resets_at: 99_000,
                severity: None,
                share_of_weekly: None,
                allowance_of_weekly: None,
            }],
            credits: None,
            resets: None,
        };
        let got: Vec<_> = limit_samples(50, &[u]).into_iter().map(|s| (s.label, s.pct, s.resets)).collect();
        assert_eq!(got, vec![("5h".into(), 61.0, 9000), ("Fable".into(), 95.0, 99_000)]);
        let s = SessionStatus {
            slug: "p-1".into(),
            session_id: "abc".into(),
            transcript: String::new(),
            account: "work".into(),
            model: String::new(),
            model_id: String::new(),
            effort: None,
            fast: false,
            cost: 3.5,
            context_percent: None,
            cache_warm: None,
            cache_expires_at: None,
            recache_tokens: None,
        };
        let one = CostSample { t: 50, session: "abc".into(), slug: "p-1".into(), account: None, cost: 3.5 };
        assert_eq!(cost_samples(50, std::slice::from_ref(&s), false), vec![one.clone()]);
        assert_eq!(cost_samples(50, &[s], true), vec![CostSample { account: Some("work".into()), ..one }]);
    }
}
