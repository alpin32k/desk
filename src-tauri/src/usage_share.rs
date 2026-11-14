//! Which sessions used the current 5-hour window, as fractions of what was
//! used in it (they add up to 1). Two ways: from each session's cost in
//! desk's history (mode "cost", here) or from Claude Code's transcripts
//! (mode "transcripts", usage_transcripts.rs).

use crate::usage_history::CostSample;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareRow {
    /// The desk session (its tile's slug); None for one outside desk.
    pub slug: Option<String>,
    /// The slug, or the project's name for a session outside desk.
    pub label: String,
    pub fraction: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Share {
    /// "cost" or "transcripts".
    pub mode: String,
    /// Largest first.
    pub rows: Vec<ShareRow>,
    /// desk was not recording when the window began: early use is missing.
    pub incomplete: bool,
}

/// Each session's growth in cost since the window began. Its cost then is
/// its last reading before the window; with none, 0 when desk was already
/// recording (the session began in the window), else its first reading in
/// the window, and the share is incomplete. `account` None counts them all.
pub fn from_costs(costs: &[CostSample], account: Option<&str>, window_start: u64, recording_since: Option<u64>) -> Share {
    let mut by_session: std::collections::BTreeMap<&str, Vec<&CostSample>> = Default::default();
    for c in costs.iter().filter(|c| account.is_none() || c.account.as_deref() == account) {
        by_session.entry(c.session.as_str()).or_default().push(c);
    }
    let recorded_before = recording_since.is_some_and(|r| r <= window_start);
    let mut incomplete = false;
    let mut used: Vec<(String, f64)> = Vec::new();
    for samples in by_session.values_mut() {
        samples.sort_by_key(|c| c.t);
        let Some(last) = samples.last() else { continue };
        let before = samples.iter().rev().find(|c| c.t <= window_start);
        let start = match before {
            Some(c) => c.cost,
            None if recorded_before => 0.0,
            None => {
                incomplete = true;
                samples[0].cost
            }
        };
        let delta = (last.cost - start).max(0.0);
        if delta > 0.0 {
            used.push((last.slug.clone(), delta));
        }
    }
    Share { mode: "cost".into(), rows: rows(used), incomplete }
}

/// Fractions of the total, largest first; `slug` set for desk sessions.
fn rows(used: Vec<(String, f64)>) -> Vec<ShareRow> {
    let total: f64 = used.iter().map(|u| u.1).sum();
    let mut rows: Vec<ShareRow> = used
        .into_iter()
        .map(|(slug, x)| ShareRow { label: slug.clone(), slug: Some(slug), fraction: x / total })
        .collect();
    rows.sort_by(|a, b| b.fraction.total_cmp(&a.fraction));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(t: u64, s: &str, cost: f64) -> CostSample {
        CostSample { t, session: s.into(), slug: s.into(), account: None, cost }
    }

    #[test]
    fn share_from_cost_deltas() {
        let s = from_costs(&[c(900, "a", 2.0), c(1500, "a", 5.0), c(1200, "b", 0.5), c(1600, "b", 1.5)], None, 1000, Some(800));
        assert!(!s.incomplete);
        assert_eq!(s.rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert!((s.rows[0].fraction - 3.0 / 4.5).abs() < 1e-9);
        assert!((s.rows[1].fraction - 1.5 / 4.5).abs() < 1e-9);
        assert_eq!(s.rows[0].slug.as_deref(), Some("a"));
        let s = from_costs(&[c(1150, "a", 2.0), c(1500, "a", 5.0)], None, 1000, Some(1100));
        assert!(s.incomplete);
        assert_eq!(s.rows[0].fraction, 1.0);
    }

    #[test]
    fn idle_sessions_have_no_row() {
        let s = from_costs(&[c(900, "a", 2.0), c(1200, "a", 2.0), c(900, "b", 1.0), c(1300, "b", 4.0)], None, 1000, Some(800));
        assert_eq!(s.rows.len(), 1);
        assert_eq!(s.rows[0].label, "b");
        assert!(from_costs(&[], None, 1000, None).rows.is_empty());
    }

    #[test]
    fn share_counts_only_the_account() {
        let w = |t, cost| CostSample { t, session: "w".into(), slug: "w".into(), account: Some("work".into()), cost };
        let o = |t, cost| CostSample { t, session: "o".into(), slug: "o".into(), account: Some("own".into()), cost };
        let s = from_costs(&[w(900, 1.0), w(1500, 3.0), o(900, 1.0), o(1500, 9.0)], Some("work"), 1000, Some(800));
        assert_eq!(s.rows, vec![ShareRow { slug: Some("w".into()), label: "w".into(), fraction: 1.0 }]);
    }
}
