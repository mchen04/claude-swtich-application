use std::io::Write;
use std::time::Duration;

use chrono::{DateTime, Utc};
use crossterm::{cursor, terminal, ExecutableCommand};
use serde::Serialize;

use std::path::PathBuf;

use crate::cli::{GlobalOpts, UsageArgs};
use crate::commands::list;
use crate::error::Result;
use crate::keychain::Keychain;
use crate::output::{emit_json, emit_text, OutputOpts};
use crate::paths::Paths;
use crate::profile::ProfileSummary;
use crate::usage::{ccusage::CcusageClient, DailyTotal};

#[derive(Debug, Serialize)]
struct UsageReport {
    mode: &'static str,
    generated_at: String,
    rows: Vec<UsageRow>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct UsageRow {
    profile: String,
    is_active: bool,
    plan: Option<String>,
    used_5h: u64,
    left_5h: String,
    burn_per_min: Option<f64>,
    weekly_used: u64,
    cost_5h: f64,
    cost_weekly: f64,
    error: Option<String>,
}

pub fn run(
    paths: &Paths,
    kc: &dyn Keychain,
    global: &GlobalOpts,
    args: &UsageArgs,
) -> Result<()> {
    let client = CcusageClient::new();
    if args.watch {
        return run_watch(paths, kc, &client, global, args);
    }
    let report = build(paths, kc, &client, args)?;
    if global.json {
        emit_json(&report)?;
    } else {
        emit_text(
            OutputOpts { json: false },
            &TextReport {
                report: &report,
                with_price: args.price,
            },
        )?;
    }
    Ok(())
}

fn build(
    paths: &Paths,
    kc: &dyn Keychain,
    client: &CcusageClient,
    args: &UsageArgs,
) -> Result<UsageReport> {
    let now = Utc::now();
    let listing = list::build(paths, kc)?;
    let mut warnings = Vec::new();
    let mut rows = Vec::new();

    let mode = mode_of(args);

    for p in &listing.profiles {
        let home = effective_home_for(paths, p);
        // ccusage refuses to start unless `<home>/projects` exists.
        std::fs::create_dir_all(home.join("projects")).ok();

        let mut row = UsageRow {
            profile: p.name.clone(),
            is_active: p.is_active,
            plan: p.plan.clone(),
            used_5h: 0,
            left_5h: "—".into(),
            burn_per_min: None,
            weekly_used: 0,
            cost_5h: 0.0,
            cost_weekly: 0.0,
            error: None,
        };

        let blocks = match client.active_blocks_for(&home) {
            Ok(b) => b,
            Err(e) => {
                let msg = condense_err(&format!("blocks: {e}"));
                warnings.push(format!("{}: {msg}", p.name));
                row.error = Some(msg);
                Vec::new()
            }
        };
        let daily = match client.daily_for(&home) {
            Ok(d) => d,
            Err(e) => {
                let msg = condense_err(&format!("daily: {e}"));
                warnings.push(format!("{}: {msg}", p.name));
                if row.error.is_none() {
                    row.error = Some(msg);
                }
                Vec::new()
            }
        };

        if let Some(b) = blocks.first() {
            row.used_5h = b.tokens_in
                + b.tokens_out
                + b.cache_creation_tokens
                + b.cache_read_tokens;
            row.left_5h = fmt_remaining(now, b.resets_at.as_deref(), b.remaining_minutes);
            row.burn_per_min = b.burn_rate_per_min;
            row.cost_5h = b.cost_usd;
        }

        match mode {
            Mode::Blocks => {
                let last = take_last(&daily, 7);
                row.weekly_used = last.iter().map(|d| d.total_tokens).sum();
                row.cost_weekly = last.iter().map(|d| d.cost_usd).sum::<f64>().max(0.0);
            }
            Mode::Daily => {
                let today = now.format("%Y-%m-%d").to_string();
                if let Some(d) = daily.iter().find(|d| d.date == today) {
                    row.weekly_used = d.total_tokens;
                    row.cost_weekly = d.cost_usd.max(0.0);
                }
            }
            Mode::Monthly => {
                let last = take_last(&daily, 30);
                row.weekly_used = last.iter().map(|d| d.total_tokens).sum();
                row.cost_weekly = last.iter().map(|d| d.cost_usd).sum::<f64>().max(0.0);
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
        mode: mode.as_str(),
        generated_at: now.to_rfc3339(),
        rows,
        warnings,
    })
}

fn run_watch(
    paths: &Paths,
    kc: &dyn Keychain,
    client: &CcusageClient,
    global: &GlobalOpts,
    args: &UsageArgs,
) -> Result<()> {
    let mut stdout = std::io::stdout();
    let _ = stdout.execute(cursor::Hide);
    let mut first = true;
    loop {
        let report = build(paths, kc, client, args)?;
        if !first {
            let _ = stdout.execute(cursor::MoveToColumn(0));
            let _ = stdout.execute(terminal::Clear(terminal::ClearType::FromCursorDown));
            let _ = stdout.execute(cursor::MoveToPreviousLine(40));
            let _ = stdout.execute(terminal::Clear(terminal::ClearType::FromCursorDown));
        }
        first = false;
        if global.json {
            serde_json::to_writer_pretty(&mut stdout, &report)?;
            writeln!(stdout, "\n(updated {})", chrono::Utc::now().to_rfc3339())?;
        } else {
            write!(
                stdout,
                "{}",
                TextReport {
                    report: &report,
                    with_price: args.price,
                }
            )?;
            writeln!(stdout, "(updated {})", chrono::Utc::now().to_rfc3339())?;
        }
        stdout.flush().ok();
        std::thread::sleep(Duration::from_millis(1000));
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Blocks,
    Daily,
    Monthly,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Mode::Blocks => "blocks",
            Mode::Daily => "daily",
            Mode::Monthly => "monthly",
        }
    }
}

fn mode_of(a: &UsageArgs) -> Mode {
    if a.daily {
        Mode::Daily
    } else if a.monthly {
        Mode::Monthly
    } else {
        Mode::Blocks
    }
}

fn take_last(daily: &[DailyTotal], n: usize) -> Vec<&DailyTotal> {
    let mut sorted: Vec<&DailyTotal> = daily.iter().collect();
    sorted.sort_by(|a, b| b.date.cmp(&a.date));
    sorted.into_iter().take(n).collect()
}

fn fmt_remaining(
    now: DateTime<Utc>,
    resets_at: Option<&str>,
    fallback_minutes: Option<u64>,
) -> String {
    if let Some(s) = resets_at {
        if let Ok(reset) = DateTime::parse_from_rfc3339(s) {
            let secs = (reset.with_timezone(&Utc) - now).num_seconds();
            if secs > 0 {
                let h = secs / 3600;
                let m = (secs % 3600) / 60;
                return format!("{h}h{m:02}m");
            }
        }
    }
    if let Some(mins) = fallback_minutes {
        if mins > 0 {
            let h = mins / 60;
            let m = mins % 60;
            return format!("{h}h{m:02}m");
        }
    }
    "—".into()
}

fn fmt_tokens(n: u64) -> String {
    if n == 0 {
        return "0 tok".to_string();
    }
    if n >= 1_000_000 {
        format!("{:.1}M tok", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{}k tok", n / 1_000)
    } else {
        format!("{n} tok")
    }
}

/// Pick which Claude home to point ccusage at for this profile.
///
/// Default = the per-profile isolated home. Once users start launching claude via
/// `cs run` / `cs shell`, that's where the jsonl accumulates. But during the
/// transition (and for users who only run `claude` directly), the per-profile
/// `projects/` is empty while `~/.claude/projects/` has the real data — for the
/// active profile we fall back to the canonical home so day-one usage isn't blank.
fn effective_home_for(paths: &Paths, p: &ProfileSummary) -> PathBuf {
    let per_profile = paths.profile_provider_home(&p.name, "claude");
    if p.is_active && projects_is_empty(&per_profile) {
        let canonical_projects = paths.claude_home.join("projects");
        if has_jsonl(&canonical_projects) {
            return paths.claude_home.clone();
        }
    }
    per_profile
}

fn projects_is_empty(home: &std::path::Path) -> bool {
    !has_jsonl(&home.join("projects"))
}

fn has_jsonl(dir: &std::path::Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e == "jsonl")
        {
            return true;
        }
        if path.is_dir() && has_jsonl(&path) {
            return true;
        }
    }
    false
}

/// ccusage on a missing home dumps a multi-line JS stack trace through stderr.
/// Keep just the first informative line so the table stays readable.
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

fn fmt_burn(rate: Option<f64>) -> String {
    match rate {
        Some(r) if r > 0.0 => format!("{}/m", r.round() as u64),
        _ => "—".into(),
    }
}

struct TextReport<'a> {
    report: &'a UsageReport,
    with_price: bool,
}

impl std::fmt::Display for TextReport<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.report.rows.is_empty() {
            writeln!(f, "(no claude profiles saved — `cs save <name>` to add one)")?;
            return Ok(());
        }
        let third = match self.report.mode {
            "daily" => "TODAY",
            "monthly" => "30D",
            _ => "WEEKLY USED",
        };
        write!(
            f,
            "{:<3}{:<18}{:<14}{:<10}{:<9}{:<18}{:<8}",
            "", "PROFILE", "5H USED", "5H LEFT", "BURN", third, "PLAN"
        )?;
        if self.with_price {
            let third_price = match self.report.mode {
                "daily" => "TODAY $",
                "monthly" => "30D $",
                _ => "WEEKLY $",
            };
            write!(f, "{:<11}{:<11}", "5H $", third_price)?;
        }
        writeln!(f)?;

        for r in &self.report.rows {
            let mark = if r.is_active { "* " } else { "  " };
            let plan = r.plan.as_deref().unwrap_or("—");
            write!(
                f,
                "{:<3}{:<18}{:<14}{:<10}{:<9}{:<18}{:<8}",
                mark,
                r.profile,
                fmt_tokens(r.used_5h),
                r.left_5h,
                fmt_burn(r.burn_per_min),
                fmt_tokens(r.weekly_used),
                plan,
            )?;
            if self.with_price {
                let cost_5h = format!("${:.2}", r.cost_5h.max(0.0));
                let cost_w = format!("${:.2}", r.cost_weekly.max(0.0));
                write!(f, "{cost_5h:<11}{cost_w:<11}")?;
            }
            writeln!(f)?;
            if let Some(err) = &r.error {
                writeln!(f, "   ↳ {err}")?;
            }
        }
        if !self.report.warnings.is_empty() {
            writeln!(f)?;
        }
        Ok(())
    }
}
