use std::cmp::Ordering;

use crate::usage::limits::UsageLimits;

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Healthy,
    AllCapped,
    Switch(String),
}

/// Pure decision: pick the best replacement profile when the active one has capped.
/// Lowest 7d utilization wins; tiebreak on lowest 5h, then by name.
pub fn decide(active: &UsageLimits, others: &[(String, UsageLimits)]) -> Decision {
    if active.five_hour.utilization < 100.0 && active.seven_day.utilization < 100.0 {
        return Decision::Healthy;
    }
    let mut candidates: Vec<&(String, UsageLimits)> = others
        .iter()
        .filter(|(_, l)| l.five_hour.utilization < 100.0 && l.seven_day.utilization < 100.0)
        .collect();
    if candidates.is_empty() {
        return Decision::AllCapped;
    }
    candidates.sort_by(|a, b| {
        a.1.seven_day
            .utilization
            .partial_cmp(&b.1.seven_day.utilization)
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                a.1.five_hour
                    .utilization
                    .partial_cmp(&b.1.five_hour.utilization)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| a.0.cmp(&b.0))
    });
    Decision::Switch(candidates[0].0.clone())
}

/// Fire a macOS user notification via `osascript`. Best-effort; no error is reported.
/// Set `CS_TEST_NO_NOTIFY=1` to suppress for tests.
pub fn notify_macos(title: &str, message: &str) {
    if std::env::var_os("CS_TEST_NO_NOTIFY").is_some() {
        return;
    }
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        escape_applescript(message),
        escape_applescript(title),
    );
    let _ = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output();
}

fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::limits::Bucket;

    fn lim(five: f64, seven: f64) -> UsageLimits {
        UsageLimits {
            five_hour: Bucket {
                utilization: five,
                resets_at: None,
            },
            seven_day: Bucket {
                utilization: seven,
                resets_at: None,
            },
            seven_day_sonnet: None,
            seven_day_opus: None,
        }
    }

    #[test]
    fn healthy_active_returns_healthy() {
        let d = decide(&lim(99.0, 50.0), &[]);
        assert_eq!(d, Decision::Healthy);
    }

    #[test]
    fn capped_active_with_room_in_b_picks_b() {
        let active = lim(100.0, 50.0);
        let others = vec![
            ("B".into(), lim(10.0, 20.0)),
            ("C".into(), lim(40.0, 80.0)),
        ];
        assert_eq!(decide(&active, &others), Decision::Switch("B".into()));
    }

    #[test]
    fn weekly_capped_active_picks_lowest_seven_day() {
        let active = lim(20.0, 100.0);
        let others = vec![
            ("Z".into(), lim(0.0, 90.0)),
            ("A".into(), lim(50.0, 30.0)),
        ];
        assert_eq!(decide(&active, &others), Decision::Switch("A".into()));
    }

    #[test]
    fn all_others_capped_returns_all_capped() {
        let active = lim(100.0, 100.0);
        let others = vec![
            ("B".into(), lim(100.0, 50.0)),
            ("C".into(), lim(40.0, 100.0)),
        ];
        assert_eq!(decide(&active, &others), Decision::AllCapped);
    }

    #[test]
    fn no_others_returns_all_capped() {
        let active = lim(100.0, 100.0);
        assert_eq!(decide(&active, &[]), Decision::AllCapped);
    }

    #[test]
    fn tiebreak_on_five_hour_then_name() {
        let active = lim(100.0, 100.0);
        // B and C tied on 7d; C has lower 5h → C wins.
        let others = vec![
            ("B".into(), lim(80.0, 50.0)),
            ("C".into(), lim(30.0, 50.0)),
        ];
        assert_eq!(decide(&active, &others), Decision::Switch("C".into()));

        // All tied → name tiebreak picks earliest.
        let others = vec![
            ("zeta".into(), lim(10.0, 10.0)),
            ("alpha".into(), lim(10.0, 10.0)),
        ];
        assert_eq!(decide(&active, &others), Decision::Switch("alpha".into()));
    }
}
