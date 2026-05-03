use std::io::Write;
use std::time::Duration;

use chrono::{DateTime, Utc};
use crossterm::{cursor, terminal, ExecutableCommand};
use serde::Serialize;

use crate::cli::{GlobalOpts, UsageArgs};
use crate::commands::list;
use crate::error::Result;
use crate::keychain::{self, Keychain};
use crate::output::{emit_json, emit_text};
use crate::paths::Paths;
use crate::profile::OauthCreds;
use crate::usage::{
    limits::{self, CACHE_MAX_AGE},
    LimitsError,
};

#[derive(Debug, Serialize)]
struct UsageReport {
    generated_at: String,
    rows: Vec<UsageRow>,
    warnings: Vec<String>,
    /// Oldest data fetch among the rendered profiles (unix seconds). `None` when
    /// every row errored before we could resolve a payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    data_fetched_at_unix: Option<u64>,
    /// True when at least one row is being served from past-TTL cache (429).
    data_stale: bool,
}

#[derive(Debug, Serialize)]
struct UsageRow {
    profile: String,
    is_active: bool,
    plan: Option<String>,
    five_h_pct_left: Option<u8>,
    five_h_resets_in: Option<String>,
    weekly_pct_left: Option<u8>,
    weekly_resets_in: Option<String>,
    weekly_sonnet_pct_left: Option<u8>,
    weekly_opus_pct_left: Option<u8>,
    error: Option<String>,
}

pub fn run(
    paths: &Paths,
    kc: &dyn Keychain,
    global: &GlobalOpts,
    args: &UsageArgs,
) -> Result<()> {
    if args.watch {
        return run_watch(paths, kc, global);
    }
    let report = build(paths, kc, CACHE_MAX_AGE)?;
    if global.json {
        emit_json(&report)?;
    } else {
        emit_text(&TextReport { report: &report })?;
    }
    Ok(())
}

fn build(paths: &Paths, kc: &dyn Keychain, max_age: Duration) -> Result<UsageReport> {
    let now = Utc::now();
    let listing = list::build(paths, kc)?;
    let mut warnings = Vec::new();
    let mut rows = Vec::new();
    let mut oldest_fetch: Option<u64> = None;
    let mut any_stale = false;

    for p in &listing.profiles {
        let mut row = UsageRow {
            profile: p.name.clone(),
            is_active: p.is_active,
            plan: p.plan.clone(),
            five_h_pct_left: None,
            five_h_resets_in: None,
            weekly_pct_left: None,
            weekly_resets_in: None,
            weekly_sonnet_pct_left: None,
            weekly_opus_pct_left: None,
            error: None,
        };

        let creds = match read_creds(kc, &p.name) {
            Ok(c) => c,
            Err(msg) => {
                warnings.push(format!("{}: {msg}", p.name));
                row.error = Some(msg);
                rows.push(row);
                continue;
            }
        };

        match limits::fetch_for(&p.name, &creds, paths, max_age) {
            Ok(outcome) => {
                let l = &outcome.limits;
                row.five_h_pct_left = Some(pct_left(l.five_hour.utilization));
                row.five_h_resets_in = resets_in(now, l.five_hour.resets_at.as_deref());
                row.weekly_pct_left = Some(pct_left(l.seven_day.utilization));
                row.weekly_resets_in = resets_in(now, l.seven_day.resets_at.as_deref());
                row.weekly_sonnet_pct_left =
                    l.seven_day_sonnet.as_ref().map(|b| pct_left(b.utilization));
                row.weekly_opus_pct_left =
                    l.seven_day_opus.as_ref().map(|b| pct_left(b.utilization));
                oldest_fetch = Some(match oldest_fetch {
                    Some(prev) => prev.min(outcome.data_fetched_at_unix),
                    None => outcome.data_fetched_at_unix,
                });
                if outcome.stale {
                    any_stale = true;
                    warnings.push(format!(
                        "{}: rate-limited; showing cached values",
                        p.name
                    ));
                }
            }
            Err(e) => {
                let msg = condense_err(&e.to_string());
                warnings.push(format!("{}: {msg}", p.name));
                row.error = Some(msg);
            }
        }

        rows.push(row);
    }

    rows.sort_by(|a, b| match (a.is_active, b.is_active) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.profile.cmp(&b.profile),
    });

    Ok(UsageReport {
        generated_at: now.to_rfc3339(),
        rows,
        warnings,
        data_fetched_at_unix: oldest_fetch,
        data_stale: any_stale,
    })
}

fn read_creds(kc: &dyn Keychain, profile: &str) -> std::result::Result<OauthCreds, String> {
    let account = keychain::profile_account(profile);
    let bytes = kc
        .read(&account)
        .map_err(|e| condense_err(&format!("keychain: {e}")))?;
    OauthCreds::parse(&bytes).map_err(|_| "no OAuth creds (API-key profile)".to_string())
}

fn run_watch(paths: &Paths, kc: &dyn Keychain, global: &GlobalOpts) -> Result<()> {
    let mut stdout = std::io::stdout();
    let _ = stdout.execute(cursor::Hide);
    let mut first = true;
    loop {
        let report = build(paths, kc, CACHE_MAX_AGE)?;
        if !first {
            let _ = stdout.execute(cursor::MoveToColumn(0));
            let _ = stdout.execute(terminal::Clear(terminal::ClearType::FromCursorDown));
            let _ = stdout.execute(cursor::MoveToPreviousLine(40));
            let _ = stdout.execute(terminal::Clear(terminal::ClearType::FromCursorDown));
        }
        first = false;
        let footer = render_footer(&report);
        if global.json {
            serde_json::to_writer_pretty(&mut stdout, &report)?;
            writeln!(stdout, "\n{footer}")?;
        } else {
            write!(stdout, "{}", TextReport { report: &report })?;
            writeln!(stdout, "{footer}")?;
        }
        stdout.flush().ok();
        std::thread::sleep(Duration::from_millis(1000));
    }
}

fn render_footer(report: &UsageReport) -> String {
    let Some(fetched) = report.data_fetched_at_unix else {
        return String::new();
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(fetched);
    let age = now.saturating_sub(fetched);
    if report.data_stale {
        return format!("(rate-limited · data {} old)", fmt_age(age));
    }
    let ttl = CACHE_MAX_AGE.as_secs();
    let next = ttl.saturating_sub(age);
    if next == 0 {
        format!("(fetched {} ago · refreshing…)", fmt_age(age))
    } else {
        format!(
            "(fetched {} ago · refreshes in {})",
            fmt_age(age),
            fmt_age(next)
        )
    }
}

fn fmt_age(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3_600 {
        let m = secs / 60;
        let s = secs % 60;
        if s == 0 {
            format!("{m}m")
        } else {
            format!("{m}m{s:02}s")
        }
    } else {
        let h = secs / 3_600;
        let m = (secs % 3_600) / 60;
        format!("{h}h{m:02}m")
    }
}

fn pct_left(utilization: f64) -> u8 {
    let used = utilization.clamp(0.0, 100.0).round() as u8;
    100u8.saturating_sub(used)
}

fn resets_in(now: DateTime<Utc>, resets_at: Option<&str>) -> Option<String> {
    let s = resets_at?;
    let reset = DateTime::parse_from_rfc3339(s).ok()?;
    let secs = (reset.with_timezone(&Utc) - now).num_seconds();
    if secs <= 0 {
        return None;
    }
    let days = secs / 86_400;
    let rest = secs % 86_400;
    let hours = rest / 3_600;
    let mins = (rest % 3_600) / 60;
    if days > 0 {
        Some(format!("{days}d{hours:02}h"))
    } else {
        Some(format!("{hours}h{mins:02}m"))
    }
}

fn condense_err(msg: &str) -> String {
    let trimmed = msg
        .lines()
        .find(|l| {
            let l = l.trim();
            !l.is_empty() && !l.starts_with("at ") && !l.starts_with("^")
        })
        .unwrap_or(msg)
        .trim()
        .to_string();
    if trimmed.len() > 160 {
        format!("{}…", &trimmed[..160])
    } else {
        trimmed
    }
}

struct TextReport<'a> {
    report: &'a UsageReport,
}

impl std::fmt::Display for TextReport<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.report.rows.is_empty() {
            writeln!(f, "(no claude profiles saved — `cs save <name>` to add one)")?;
            return Ok(());
        }
        writeln!(
            f,
            "{:<3}{:<18}{:<10}{:<13}{:<10}{:<13}{:<8}",
            "", "PROFILE", "5H LEFT", "5H RESETS", "7D LEFT", "7D RESETS", "PLAN"
        )?;
        for r in &self.report.rows {
            let mark = if r.is_active { "* " } else { "  " };
            let plan = r.plan.as_deref().unwrap_or("—");
            let five_pct = r
                .five_h_pct_left
                .map(|p| format!("{p}%"))
                .unwrap_or_else(|| "—".into());
            let five_reset = r.five_h_resets_in.clone().unwrap_or_else(|| "—".into());
            let week_pct = r
                .weekly_pct_left
                .map(|p| format!("{p}%"))
                .unwrap_or_else(|| "—".into());
            let week_reset = r.weekly_resets_in.clone().unwrap_or_else(|| "—".into());
            write!(
                f,
                "{:<3}{:<18}{:<10}{:<13}{:<10}{:<13}{:<8}",
                mark, r.profile, five_pct, five_reset, week_pct, week_reset, plan
            )?;
            if let Some(err) = &r.error {
                write!(f, "    ↳ {err}")?;
            }
            writeln!(f)?;
        }
        if !self.report.warnings.is_empty() {
            writeln!(f)?;
            for w in &self.report.warnings {
                writeln!(f, "warning: {w}")?;
            }
        }
        Ok(())
    }
}

impl From<LimitsError> for crate::error::Error {
    fn from(e: LimitsError) -> Self {
        crate::error::Error::Other(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pct_left_inverts_utilization() {
        assert_eq!(pct_left(0.0), 100);
        assert_eq!(pct_left(37.0), 63);
        assert_eq!(pct_left(37.4), 63);
        assert_eq!(pct_left(37.6), 62);
        assert_eq!(pct_left(100.0), 0);
        assert_eq!(pct_left(250.0), 0);
        assert_eq!(pct_left(-5.0), 100);
    }

    #[test]
    fn fmt_age_compact_buckets() {
        assert_eq!(fmt_age(0), "0s");
        assert_eq!(fmt_age(12), "12s");
        assert_eq!(fmt_age(59), "59s");
        assert_eq!(fmt_age(60), "1m");
        assert_eq!(fmt_age(125), "2m05s");
        assert_eq!(fmt_age(3_600), "1h00m");
        assert_eq!(fmt_age(3_725), "1h02m");
    }

    #[test]
    fn resets_in_handles_past_and_future() {
        let now = DateTime::parse_from_rfc3339("2026-05-02T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        // 2 hours and 14 minutes ahead.
        let s = "2026-05-02T14:14:00Z";
        assert_eq!(resets_in(now, Some(s)).as_deref(), Some("2h14m"));
        // Past — returns None.
        assert!(resets_in(now, Some("2026-05-01T00:00:00Z")).is_none());
        // Multi-day.
        assert_eq!(
            resets_in(now, Some("2026-05-06T16:00:00Z")).as_deref(),
            Some("4d04h")
        );
    }
}
